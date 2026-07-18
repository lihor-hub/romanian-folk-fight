//! Deterministic human character-definition generation.

use std::fmt;

use super::{
    BodyRegion, CHARACTER_DEFINITION_VERSION, CatalogError, CharacterCatalog, CharacterDefinition,
    CulturalProfile, PartId, PartSelections, PlayerAppearance, SkeletonFamily,
};

/// One weighted profile-controlled candidate for a semantic character slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightedPart {
    pub id: PartId,
    pub weight: u32,
}

impl WeightedPart {
    pub const fn new(id: PartId, weight: u32) -> Self {
        Self { id, weight }
    }
}

/// Candidate choices for one semantic character slot.
///
/// A slot with one candidate is an identity lock for named encounters. A
/// slot omitted for facial hair, waist, or accessories leaves that optional
/// selection empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationSlot {
    pub region: BodyRegion,
    pub candidates: Vec<WeightedPart>,
}

impl GenerationSlot {
    pub fn new(region: BodyRegion, candidates: Vec<WeightedPart>) -> Self {
        Self { region, candidates }
    }
}

/// The authored policy for a deterministic generated fighter.
///
/// Candidate IDs and weights belong to the profile, while the catalog
/// determines whether those candidates can actually occupy the slot for the
/// requested skeleton and culture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationProfile {
    pub skeleton: SkeletonFamily,
    pub culture: CulturalProfile,
    pub appearance: PlayerAppearance,
    pub slots: Vec<GenerationSlot>,
}

impl GenerationProfile {
    pub fn new(
        skeleton: SkeletonFamily,
        culture: CulturalProfile,
        appearance: PlayerAppearance,
        slots: Vec<GenerationSlot>,
    ) -> Self {
        Self {
            skeleton,
            culture,
            appearance,
            slots,
        }
    }
}

/// Reasons a generated definition cannot be safely assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationError {
    Catalog(CatalogError),
    MissingRequiredSlot { region: BodyRegion },
    DuplicateSlot { region: BodyRegion },
    NoCompatibleCandidates { region: BodyRegion },
    ZeroTotalWeight { region: BodyRegion },
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "character catalog error: {error}"),
            Self::MissingRequiredSlot { region } => {
                write!(
                    formatter,
                    "generation profile is missing required `{region:?}` slot"
                )
            }
            Self::DuplicateSlot { region } => {
                write!(formatter, "generation profile repeats `{region:?}` slot")
            }
            Self::NoCompatibleCandidates { region } => write!(
                formatter,
                "generation profile has no compatible candidates for `{region:?}`"
            ),
            Self::ZeroTotalWeight { region } => write!(
                formatter,
                "generation profile has zero total candidate weight for `{region:?}`"
            ),
        }
    }
}

impl std::error::Error for GenerationError {}

impl From<CatalogError> for GenerationError {
    fn from(error: CatalogError) -> Self {
        Self::Catalog(error)
    }
}

/// Builds the catalog's explicit known-good human fallback definition.
///
/// This adapter is deliberately separate from [`generate_character`]:
/// generation never substitutes a fallback for an invalid catalog or profile.
pub fn fallback_human(catalog: &CharacterCatalog) -> Result<CharacterDefinition, GenerationError> {
    Ok(catalog.resolve_known_good_human()?.definition().clone())
}

/// Generates a stable human definition for one seed and authored profile.
///
/// The private selector is intentionally dependency-free and candidates are
/// sorted by stable ID before every weighted draw, so catalog `HashMap` order
/// cannot influence a resolved definition.
pub fn generate_character(
    seed: u64,
    profile: &GenerationProfile,
    catalog: &CharacterCatalog,
) -> Result<CharacterDefinition, GenerationError> {
    catalog.validate()?;

    let mut selector = SeededSelector::new(seed);
    let body = select_required(BodyRegion::Body, profile, catalog, &mut selector)?;
    let face = select_required(BodyRegion::Face, profile, catalog, &mut selector)?;
    let hair = select_required(BodyRegion::Hair, profile, catalog, &mut selector)?;
    let torso = select_required(BodyRegion::Torso, profile, catalog, &mut selector)?;
    let legs = select_required(BodyRegion::Legs, profile, catalog, &mut selector)?;
    let feet = select_required(BodyRegion::Feet, profile, catalog, &mut selector)?;
    let facial_hair = select_optional(BodyRegion::FacialHair, profile, catalog, &mut selector)?;
    let waist = select_optional(BodyRegion::Waist, profile, catalog, &mut selector)?;
    let accessories = profile
        .slots
        .iter()
        .filter(|slot| slot.region == BodyRegion::Accessory)
        .map(|slot| select(slot, profile, catalog, &mut selector))
        .collect::<Result<Vec<_>, _>>()?;

    let definition = CharacterDefinition {
        version: CHARACTER_DEFINITION_VERSION,
        seed: Some(seed),
        skeleton: profile.skeleton,
        culture: profile.culture.clone(),
        parts: PartSelections {
            body,
            face,
            hair,
            facial_hair,
            torso,
            legs,
            feet,
            waist,
            accessories,
        },
        appearance: profile.appearance,
    };

    catalog.resolve(&definition)?;
    Ok(definition)
}

fn select_required(
    region: BodyRegion,
    profile: &GenerationProfile,
    catalog: &CharacterCatalog,
    selector: &mut SeededSelector,
) -> Result<PartId, GenerationError> {
    let slot = one_slot(region, profile)?.ok_or(GenerationError::MissingRequiredSlot { region })?;
    select(slot, profile, catalog, selector)
}

fn select_optional(
    region: BodyRegion,
    profile: &GenerationProfile,
    catalog: &CharacterCatalog,
    selector: &mut SeededSelector,
) -> Result<Option<PartId>, GenerationError> {
    one_slot(region, profile)?
        .map(|slot| select(slot, profile, catalog, selector))
        .transpose()
}

fn one_slot(
    region: BodyRegion,
    profile: &GenerationProfile,
) -> Result<Option<&GenerationSlot>, GenerationError> {
    let mut slots = profile.slots.iter().filter(|slot| slot.region == region);
    let slot = slots.next();
    if slots.next().is_some() {
        return Err(GenerationError::DuplicateSlot { region });
    }
    Ok(slot)
}

fn select(
    slot: &GenerationSlot,
    profile: &GenerationProfile,
    catalog: &CharacterCatalog,
    selector: &mut SeededSelector,
) -> Result<PartId, GenerationError> {
    let mut compatible = catalog
        .compatible_parts(slot.region, profile.skeleton, &profile.culture.tags)
        .into_iter()
        .filter_map(|part| {
            slot.candidates
                .iter()
                .find(|candidate| candidate.id == part.id)
                .map(|candidate| (&part.id, candidate.weight))
        })
        .collect::<Vec<_>>();
    compatible.sort_unstable_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));
    if compatible.is_empty() {
        return Err(GenerationError::NoCompatibleCandidates {
            region: slot.region,
        });
    }

    let total_weight = compatible
        .iter()
        .map(|(_, weight)| u64::from(*weight))
        .sum::<u64>();
    if total_weight == 0 {
        return Err(GenerationError::ZeroTotalWeight {
            region: slot.region,
        });
    }

    if compatible.len() == 1 {
        return Ok(compatible[0].0.clone());
    }

    let mut ticket = selector.next_u64() % total_weight;
    for (id, weight) in compatible {
        if ticket < u64::from(weight) {
            return Ok(id.clone());
        }
        ticket -= u64::from(weight);
    }

    unreachable!("a positive weighted candidate list always contains the selected ticket")
}

/// A fixed SplitMix64 stream. Keep this private: golden tests pin its output
/// through the generated stable IDs, not as a public randomness contract.
struct SeededSelector {
    state: u64,
}

impl SeededSelector {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GenerationError, GenerationProfile, GenerationSlot, WeightedPart, fallback_human,
        generate_character,
    };
    use crate::character::{
        BodyRegion, CharacterCatalog, CulturalProfile, PartId, PlayerAppearance, SkeletonFamily,
    };

    fn catalog() -> CharacterCatalog {
        CharacterCatalog::from_json(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON")
    }

    fn catalog_with_reversed_part_order() -> CharacterCatalog {
        let mut document: serde_json::Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        document["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .reverse();

        CharacterCatalog::from_json(&document.to_string())
            .expect("reordered human foundation fixture is valid JSON")
    }

    fn catalog_with_incompatible_hair() -> CharacterCatalog {
        let mut document: serde_json::Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        document["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.hair.tied.v1")
            .expect("fixture has tied hair")["cultural_tags"] =
            serde_json::json!(["generic_fantasy"]);

        CharacterCatalog::from_json(&document.to_string())
            .expect("mixed-culture human foundation fixture is valid JSON")
    }

    fn id(value: &str) -> PartId {
        PartId::new(value).expect("test IDs are non-blank")
    }

    fn fixed(region: BodyRegion, part_id: &str) -> GenerationSlot {
        GenerationSlot::new(region, vec![WeightedPart::new(id(part_id), 1)])
    }

    fn human_profile() -> GenerationProfile {
        GenerationProfile::new(
            SkeletonFamily::Human,
            CulturalProfile {
                tags: vec!["romanian".to_owned()],
            },
            PlayerAppearance::default(),
            vec![
                fixed(BodyRegion::Body, "human.body.foundation.v1"),
                fixed(BodyRegion::Face, "human.face.default.v1"),
                GenerationSlot::new(
                    BodyRegion::Hair,
                    vec![
                        WeightedPart::new(id("human.hair.braided.v1"), 1),
                        WeightedPart::new(id("human.hair.long.v1"), 1),
                        WeightedPart::new(id("human.hair.short.v1"), 1),
                        WeightedPart::new(id("human.hair.tied.v1"), 1),
                    ],
                ),
                fixed(BodyRegion::Torso, "human.torso.linen.v1"),
                fixed(BodyRegion::Legs, "human.legs.itari.v1"),
                fixed(BodyRegion::Feet, "human.feet.opinci.v1"),
            ],
        )
    }

    #[test]
    fn identical_inputs_produce_byte_for_byte_equal_definitions() {
        let catalog = catalog();
        let profile = human_profile();

        let first = generate_character(0xabad_1dea, &profile, &catalog).unwrap();
        let second = generate_character(0xabad_1dea, &profile, &catalog).unwrap();

        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
    }

    #[test]
    fn different_seeds_vary_an_unlocked_choice() {
        let catalog = catalog();
        let profile = human_profile();
        let first = generate_character(0, &profile, &catalog).unwrap();

        assert!(
            (1..64).any(|seed| {
                generate_character(seed, &profile, &catalog)
                    .unwrap()
                    .parts
                    .hair
                    != first.parts.hair
            }),
            "at least one seed must vary the unlocked hair choice"
        );
    }

    #[test]
    fn catalog_part_order_cannot_change_seeded_choices() {
        let ordered_catalog = catalog();
        let reordered_catalog = catalog_with_reversed_part_order();
        let profile = human_profile();

        for seed in 0..64 {
            assert_eq!(
                generate_character(seed, &profile, &ordered_catalog),
                generate_character(seed, &profile, &reordered_catalog),
                "catalog record order changed generated definition for seed {seed}"
            );
        }
    }

    #[test]
    fn omitted_optional_waist_stays_empty_until_the_renderer_supports_it() {
        let catalog = catalog();

        for seed in 0..64 {
            let hero = generate_character(seed, &human_profile(), &catalog).unwrap();
            assert_eq!(hero.parts.waist, None);
        }
    }

    #[test]
    fn incompatible_cultural_tags_are_never_selected() {
        let catalog = catalog();
        let mut profile = human_profile();
        profile.culture.tags = vec!["generic_fantasy".to_owned()];

        assert_eq!(
            generate_character(7, &profile, &catalog),
            Err(GenerationError::NoCompatibleCandidates {
                region: BodyRegion::Body,
            })
        );
    }

    #[test]
    fn incompatible_candidates_are_filtered_when_compatible_choices_remain() {
        let catalog = catalog_with_incompatible_hair();
        let profile = human_profile();

        for seed in 0..64 {
            let definition = generate_character(seed, &profile, &catalog).unwrap();
            assert_ne!(definition.parts.hair.as_str(), "human.hair.tied.v1");
        }
    }

    #[test]
    fn fallback_adapter_returns_the_catalog_owned_human_definition() {
        let catalog = catalog();
        let definition = fallback_human(&catalog).unwrap();

        assert_eq!(definition.seed, None);
        assert_eq!(definition.parts, *catalog.known_good_human());
        assert!(catalog.resolve(&definition).is_ok());
    }

    #[test]
    fn seeded_selector_sequence_is_pinned() {
        let catalog = catalog();
        let profile = human_profile();
        let generated = (0..4)
            .map(|seed| generate_character(seed, &profile, &catalog).unwrap())
            .map(|definition| definition.parts.hair.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            generated,
            [
                "human.hair.tied.v1",
                "human.hair.long.v1",
                "human.hair.short.v1",
                "human.hair.long.v1",
            ]
        );
    }
}
