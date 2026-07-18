//! Validated, data-driven part records for the modular character pipeline.

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use serde::Deserialize;

use super::{CharacterDefinition, PartId, PartSelections, SkeletonFamily};

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
}

/// A validated lookup table for character parts and a safe human fallback.
#[derive(Debug, Clone)]
pub struct CharacterCatalog {
    version: u32,
    parts: HashMap<PartId, PartRecord>,
    duplicate_ids: Vec<PartId>,
    known_good_human: PartSelections,
}

impl CharacterCatalog {
    /// Parses one catalog document without discarding duplicate authored IDs.
    ///
    /// Call [`Self::validate`] before using a catalog from untrusted or newly
    /// authored data. [`Self::resolve`] always performs that validation.
    pub fn from_json(json: &str) -> Result<Self, CatalogError> {
        let document: CatalogDocument = serde_json::from_str(json)
            .map_err(|error| CatalogError::InvalidJson(error.to_string()))?;
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

    /// Returns the resolved record for a stable part ID, when present.
    pub fn part(&self, id: &PartId) -> Option<&PartRecord> {
        self.parts.get(id)
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
            for companion in &part.companions {
                if !self.parts.contains_key(companion) {
                    return Err(CatalogError::MissingCompanionPart {
                        part_id: part.id.clone(),
                        companion: companion.clone(),
                    });
                }
            }
        }

        self.validate_selections(&self.known_good_human)
    }

    /// Resolves a stable definition into the records needed by the renderer.
    pub fn resolve(
        &self,
        definition: &CharacterDefinition,
    ) -> Result<ResolvedCharacter, CatalogError> {
        self.validate()?;

        let selections = selected_parts(&definition.parts);
        let selected_ids = selections
            .iter()
            .map(|(id, _)| (*id).clone())
            .collect::<HashSet<_>>();
        let mut parts = HashMap::with_capacity(selections.len());

        for (id, expected_region) in selections {
            let part = self
                .part(id)
                .ok_or_else(|| CatalogError::UnknownPart(id.clone()))?;
            if part.region != expected_region {
                return Err(CatalogError::WrongRegion {
                    part_id: id.clone(),
                    expected: expected_region,
                    actual: part.region,
                });
            }
            if !part.skeletons.contains(&definition.skeleton) {
                return Err(CatalogError::IncompatibleSkeleton {
                    part_id: id.clone(),
                    skeleton: definition.skeleton,
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

        Ok(ResolvedCharacter {
            definition: definition.clone(),
            parts,
        })
    }

    const REQUIRED_HUMAN_REGIONS: [BodyRegion; 6] = [
        BodyRegion::Body,
        BodyRegion::Face,
        BodyRegion::Hair,
        BodyRegion::Torso,
        BodyRegion::Legs,
        BodyRegion::Feet,
    ];

    fn validate_selections(&self, selections: &PartSelections) -> Result<(), CatalogError> {
        for (id, expected_region) in selected_parts(selections) {
            let part = self
                .part(id)
                .ok_or_else(|| CatalogError::UnknownPart(id.clone()))?;
            if part.region != expected_region {
                return Err(CatalogError::WrongRegion {
                    part_id: id.clone(),
                    expected: expected_region,
                    actual: part.region,
                });
            }
        }

        Ok(())
    }
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
    DuplicatePartId(PartId),
    MissingRequiredRegion(BodyRegion),
    MissingCompanionPart {
        part_id: PartId,
        companion: PartId,
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

    use crate::character::{CharacterDefinition, PartId, PlayerAppearance, SkeletonFamily};

    use super::{BodyRegion, CatalogError, CharacterCatalog};

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
    fn known_good_human_resolves_all_selected_parts() {
        let catalog = fixture();
        let definition = CharacterDefinition::legacy_human(PlayerAppearance::default());

        assert_eq!(catalog.validate(), Ok(()));
        assert_eq!(catalog.known_good_human(), &definition.parts);

        let resolved = catalog.resolve(&definition).unwrap();
        assert_eq!(resolved.definition(), &definition);
        assert_eq!(resolved.parts().len(), 7);
    }
}
