//! Stable, serializable identities for the modular character pipeline.

use std::{fmt, ops::Deref};

use serde::{Deserialize, Deserializer, Serialize};

use super::{HairStyle, PlayerAppearance};

/// Version of the first stable modular-character schema.
pub const CHARACTER_DEFINITION_VERSION: u32 = 1;

/// A stable catalog key for one independently selectable character part.
///
/// IDs are owned because they cross save-file and asset-catalog boundaries.
/// Construction and deserialization reject blank values so invalid catalog
/// references cannot enter a resolved definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct PartId(String);

impl PartId {
    /// Builds a stable part ID from a non-blank catalog key.
    pub fn new(value: impl Into<String>) -> Result<Self, PartIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PartIdError);
        }

        Ok(Self(value))
    }

    /// Borrows the catalog key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PartId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for PartId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for PartId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PartId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The error returned when a [`PartId`] has no meaningful catalog key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartIdError;

impl fmt::Display for PartIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("part IDs must not be blank")
    }
}

impl std::error::Error for PartIdError {}

/// The anatomical rig family that supplies legal character attachment slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkeletonFamily {
    Human,
}

/// Cultural constraints applied while selecting compatible wardrobe parts.
///
/// Tags deliberately remain open-ended strings during the human tracer bullet;
/// the catalog and generator can add regional, role, status, and folklore tags
/// without changing the persisted definition schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CulturalProfile {
    pub tags: Vec<String>,
}

/// The semantic part slots that make a resolved character visible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartSelections {
    pub body: PartId,
    pub face: PartId,
    pub hair: PartId,
    pub facial_hair: Option<PartId>,
    pub torso: PartId,
    pub legs: PartId,
    pub feet: PartId,
    pub waist: Option<PartId>,
    pub accessories: Vec<PartId>,
}

/// A versioned, resolved identity shared by player, generated, and named
/// fighters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterDefinition {
    pub version: u32,
    pub seed: Option<u64>,
    pub skeleton: SkeletonFamily,
    pub culture: CulturalProfile,
    pub parts: PartSelections,
    /// Temporary palette/proportion bridge for existing save and UI contracts.
    pub appearance: PlayerAppearance,
}

impl CharacterDefinition {
    /// Reconstructs the first resolved human identity from the legacy player
    /// appearance contract.
    pub fn legacy_human(appearance: PlayerAppearance) -> Self {
        Self {
            version: CHARACTER_DEFINITION_VERSION,
            seed: None,
            skeleton: SkeletonFamily::Human,
            culture: CulturalProfile {
                tags: vec!["romanian".to_owned()],
            },
            parts: PartSelections {
                body: legacy_id("human.body.foundation.v1"),
                face: legacy_id("human.face.default.v1"),
                hair: legacy_hair_id(appearance.hair),
                facial_hair: None,
                torso: legacy_id("human.torso.linen.v1"),
                legs: legacy_id("human.legs.itari.v1"),
                feet: legacy_id("human.feet.opinci.v1"),
                // The foundation renderer has no independent waist layer yet.
                // Keep the optional schema slot empty until a selected chimir
                // can affect visible output instead of claiming an identity
                // that the compatibility rig silently ignores.
                waist: None,
                accessories: Vec::new(),
            },
            appearance,
        }
    }
}

fn legacy_id(value: &'static str) -> PartId {
    // Every caller supplies a private, vetted non-blank literal. Constructing
    // the owned key directly keeps legacy migration free of a panic path.
    PartId(value.to_owned())
}

fn legacy_hair_id(hair: HairStyle) -> PartId {
    let id = match hair {
        HairStyle::Braided => "human.hair.braided.v1",
        HairStyle::Long => "human.hair.long.v1",
        HairStyle::Short => "human.hair.short.v1",
        HairStyle::Tied => "human.hair.tied.v1",
    };
    legacy_id(id)
}

#[cfg(test)]
mod tests {
    use super::{CharacterDefinition, PartId, SkeletonFamily};
    use crate::character::PlayerAppearance;

    #[test]
    fn part_ids_reject_blank_values() {
        assert!(PartId::new("").is_err());
        assert!(PartId::new("   ").is_err());
        assert!(serde_json::from_str::<PartId>(r#""   ""#).is_err());
    }

    #[test]
    fn legacy_appearance_uses_the_human_skeleton() {
        let appearance = PlayerAppearance::default();
        let definition = CharacterDefinition::legacy_human(appearance);

        assert_eq!(definition.skeleton, SkeletonFamily::Human);
        assert_eq!(definition.appearance, appearance);
    }

    #[test]
    fn legacy_appearance_preserves_the_selected_hair_identity() {
        let braided = CharacterDefinition::legacy_human(PlayerAppearance::default());
        let long = CharacterDefinition::legacy_human(PlayerAppearance {
            hair: crate::character::HairStyle::Long,
            ..PlayerAppearance::default()
        });

        assert_ne!(braided.parts.hair, long.parts.hair);
    }

    #[test]
    fn every_legacy_hair_style_maps_to_a_nonblank_stable_id() {
        for hair in [
            crate::character::HairStyle::Braided,
            crate::character::HairStyle::Long,
            crate::character::HairStyle::Short,
            crate::character::HairStyle::Tied,
        ] {
            let definition = CharacterDefinition::legacy_human(PlayerAppearance {
                hair,
                ..PlayerAppearance::default()
            });

            assert!(!definition.parts.hair.as_str().trim().is_empty());
        }
    }

    #[test]
    fn definition_round_trip_preserves_resolved_ids() {
        let mut definition = CharacterDefinition::legacy_human(PlayerAppearance::default());
        definition.parts.facial_hair = Some(PartId::new("human.facial_hair.moustache.v1").unwrap());
        definition.parts.accessories = vec![
            PartId::new("human.accessory.brass_button.v1").unwrap(),
            PartId::new("human.accessory.woven_bracelet.v1").unwrap(),
        ];
        let json = serde_json::to_string(&definition).unwrap();

        assert_eq!(
            serde_json::from_str::<CharacterDefinition>(&json).unwrap(),
            definition
        );
    }
}
