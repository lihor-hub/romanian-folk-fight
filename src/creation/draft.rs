//! Pure hero-creation rules for the character creation screen.
//!
//! No ECS systems here: [`CharacterDraft`] is a plain value type (registered
//! as a `Resource` by the plugin) so the allocation invariants, preset data,
//! and appearance cycling remain unit-testable without a `World`.

use bevy::prelude::*;

use crate::character::{
    AccentColor, Attributes, BodyBuild, BodyRegion, CHARACTER_DEFINITION_VERSION, CatalogError,
    CharacterCatalog, CharacterDefinition, CulturalProfile, HairStyle, PartId, PartSelections,
    PlayerAppearance, SkeletonFamily, SkinTone, bundled_human_catalog,
};
use crate::items::ItemId;
// Re-exported so existing `creation::AttributeKind` users keep working after
// the enum moved to `character` (its canonical home, next to `Attributes`).
pub use crate::character::AttributeKind;

/// Curated cycling list of Romanian folk hero names. Free-text name entry is
/// out of scope (Bevy UI has no text-input widget, notably in the browser
/// build), so the custom hero cycles through this list with arrows instead.
pub const FOLK_NAMES: &[&str] = &[
    "Făt-Frumos",
    "Greuceanu",
    "Prâslea",
    "Ileana Cosânzeana",
    "Aprodul Purice",
    "Păcală",
];

/// Free attribute points to distribute on top of the base values. #128
/// raised this from 10 (four-attribute era) to 16 so eight attributes stay
/// meaningfully spendable while a level-1 hero remains viable against ladder
/// fight 1 (see `combat::engine`'s fixed-seed survivability test); #149 may
/// retune it.
pub const FREE_POINTS: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorPartField {
    Body,
    Face,
    Hair,
    Wardrobe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WardrobeChoice {
    Haiduc,
    Cioban,
}

impl WardrobeChoice {
    pub const ALL: [Self; 2] = [Self::Haiduc, Self::Cioban];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Haiduc => "Haiduc",
            Self::Cioban => "Cioban",
        }
    }

    /// Applies all authored clothing slots as one indivisible selection.
    pub fn apply(
        self,
        parts: &mut PartSelections,
        catalog: &CharacterCatalog,
    ) -> Result<(), CatalogError> {
        let (torso, legs, feet) = match self {
            Self::Haiduc => (
                "human.torso.ie_altita.v1",
                "human.legs.itari.v1",
                "human.feet.opinci.v1",
            ),
            Self::Cioban => (
                "human.torso.camasa_ciobaneasca.v1",
                "human.legs.cioareci.v1",
                "human.feet.opinci.v1",
            ),
        };
        let torso = catalog_id(catalog, BodyRegion::Torso, torso)?;
        let legs = catalog_id(catalog, BodyRegion::Legs, legs)?;
        let feet = catalog_id(catalog, BodyRegion::Feet, feet)?;
        parts.torso = torso;
        parts.legs = legs;
        parts.feet = feet;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct CreatorPartOption {
    id: &'static str,
    label: &'static str,
}

const BODY_OPTIONS: [CreatorPartOption; 2] = [
    CreatorPartOption {
        id: "human.body.zvelt.v1",
        label: "Zvelt",
    },
    CreatorPartOption {
        id: "human.body.vanjos.v1",
        label: "Vânjos",
    },
];
const FACE_OPTIONS: [CreatorPartOption; 2] = [
    CreatorPartOption {
        id: "human.face.haiduc.v1",
        label: "Haiduc",
    },
    CreatorPartOption {
        id: "human.face.cioban.v1",
        label: "Cioban",
    },
];
const HAIR_OPTIONS: [CreatorPartOption; 3] = [
    CreatorPartOption {
        id: "human.hair.plete.v1",
        label: "Plete",
    },
    CreatorPartOption {
        id: "human.hair.prins.v1",
        label: "Prins",
    },
    CreatorPartOption {
        id: "human.hair.scurt.v1",
        label: "Scurt",
    },
];

/// The five starting points on the hero-creation path chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroChoice {
    Custom,
    Preset(HeroPreset),
}

impl HeroChoice {
    pub const ALL: [Self; 5] = [
        Self::Custom,
        Self::Preset(HeroPreset::Haiducul),
        Self::Preset(HeroPreset::Voinicul),
        Self::Preset(HeroPreset::Ciobanul),
        Self::Preset(HeroPreset::UceniculSolomonar),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Custom => "Personalizat",
            Self::Preset(preset) => preset.name(),
        }
    }
}

/// The four folklore-inspired starter hero templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroPreset {
    Haiducul,
    Voinicul,
    Ciobanul,
    UceniculSolomonar,
}

impl HeroPreset {
    pub const ALL: [Self; 4] = [
        Self::Haiducul,
        Self::Voinicul,
        Self::Ciobanul,
        Self::UceniculSolomonar,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Haiducul => "Haiducul",
            Self::Voinicul => "Voinicul",
            Self::Ciobanul => "Ciobanul",
            Self::UceniculSolomonar => "Ucenicul Solomonar",
        }
    }

    pub fn flavor(self) -> &'static str {
        match self {
            Self::Haiducul => "Iute, norocos și mereu gata să lovească din mișcare.",
            Self::Voinicul => "Erou echilibrat, cu putere și rezistență de drum lung.",
            Self::Ciobanul => "Statornic și greu de clintit, crescut printre munți și furtuni.",
            Self::UceniculSolomonar => {
                "Sprijină lupta pe noroc, răbdare și o minte umblată prin taine."
            }
        }
    }

    /// Eight-attribute preset spreads (#128). Every spread totals
    /// [`AttributeKind::base_total`]` + `[`FREE_POINTS`] — the exact budget
    /// a custom hero has — pinned by this module's preset-budget test.
    /// Voinicul and Ciobanul are deliberate `magie == 0` non-casters.
    pub fn attributes(self) -> Attributes {
        match self {
            Self::Haiducul => Attributes {
                putere: 2,
                agilitate: 4,
                vitalitate: 2,
                noroc: 4,
                atac: 5,
                aparare: 2,
                carisma: 3,
                magie: 1,
            },
            Self::Voinicul => Attributes {
                putere: 4,
                agilitate: 3,
                vitalitate: 4,
                noroc: 2,
                atac: 4,
                aparare: 4,
                carisma: 2,
                magie: 0,
            },
            Self::Ciobanul => Attributes {
                putere: 3,
                agilitate: 2,
                vitalitate: 6,
                noroc: 2,
                atac: 2,
                aparare: 5,
                carisma: 3,
                magie: 0,
            },
            Self::UceniculSolomonar => Attributes {
                putere: 2,
                agilitate: 2,
                vitalitate: 4,
                noroc: 3,
                atac: 2,
                aparare: 2,
                carisma: 3,
                magie: 5,
            },
        }
    }

    pub fn appearance(self) -> PlayerAppearance {
        match self {
            Self::Haiducul => PlayerAppearance {
                skin_tone: SkinTone::Olive,
                build: BodyBuild::Lean,
                hair: HairStyle::Long,
                accent: AccentColor::Forest,
            },
            Self::Voinicul => PlayerAppearance {
                skin_tone: SkinTone::Warm,
                build: BodyBuild::Powerful,
                hair: HairStyle::Short,
                accent: AccentColor::Crimson,
            },
            Self::Ciobanul => PlayerAppearance {
                skin_tone: SkinTone::Fair,
                build: BodyBuild::Sturdy,
                hair: HairStyle::Tied,
                accent: AccentColor::Gold,
            },
            Self::UceniculSolomonar => PlayerAppearance {
                skin_tone: SkinTone::Deep,
                build: BodyBuild::Balanced,
                hair: HairStyle::Braided,
                accent: AccentColor::Storm,
            },
        }
    }

    pub fn starter_items(self) -> &'static [ItemId] {
        match self {
            Self::Haiducul => &[ItemId::ToporDePadurar, ItemId::OpinciIuti],
            Self::Voinicul => &[ItemId::BataCiobaneasca, ItemId::ScutDeLemn],
            Self::Ciobanul => &[
                ItemId::BataCiobaneasca,
                ItemId::CaciulaDeOaie,
                ItemId::CojocGros,
            ],
            Self::UceniculSolomonar => &[
                ItemId::BataCiobaneasca,
                ItemId::IeDescantata,
                ItemId::CaciulaDeOaie,
            ],
        }
    }
}

/// The in-progress character build on the creation screen. Fields are private
/// so every mutation goes through methods that uphold the invariants.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct CharacterDraft {
    choice: HeroChoice,
    custom_name_index: usize,
    attributes: Attributes,
    definition: CharacterDefinition,
    wardrobe: WardrobeChoice,
}

impl Default for CharacterDraft {
    fn default() -> Self {
        match bundled_human_catalog().and_then(Self::default_with_catalog) {
            Ok(draft) => draft,
            Err(error) => {
                panic!("bundled catalog cannot initialize character creation: {error}")
            }
        }
    }
}

impl CharacterDraft {
    pub fn default_with_catalog(catalog: &CharacterCatalog) -> Result<Self, CatalogError> {
        let draft = Self {
            choice: HeroChoice::Custom,
            custom_name_index: 0,
            attributes: Attributes::default(),
            definition: default_definition(catalog)?,
            wardrobe: WardrobeChoice::Haiduc,
        };
        catalog.resolve(draft.definition())?;
        Ok(draft)
    }

    /// The currently selected hero choice.
    pub fn choice(&self) -> HeroChoice {
        self.choice
    }

    /// Apply one starter choice, resetting stats and appearance to that
    /// template while keeping the draft editable afterward.
    pub fn select_choice(
        &mut self,
        choice: HeroChoice,
        catalog: &CharacterCatalog,
    ) -> Result<(), CatalogError> {
        let (attributes, definition, wardrobe) = match choice {
            HeroChoice::Custom => (
                Attributes::default(),
                default_definition(catalog)?,
                WardrobeChoice::Haiduc,
            ),
            HeroChoice::Preset(preset) => (
                preset.attributes(),
                definition_for_preset(preset, catalog)?,
                wardrobe_for_preset(preset),
            ),
        };
        self.choice = choice;
        self.attributes = attributes;
        self.definition = definition;
        self.wardrobe = wardrobe;
        Ok(())
    }

    /// The name shown on the screen and confirmed into the player resource.
    pub fn name(&self) -> &'static str {
        match self.choice {
            HeroChoice::Custom => FOLK_NAMES[self.custom_name_index],
            HeroChoice::Preset(preset) => preset.name(),
        }
    }

    /// Short flavor copy for the selected path.
    pub fn description(&self) -> &'static str {
        match self.choice {
            HeroChoice::Custom => {
                "Punctele și înfățișarea pornesc de la zero. Tu îți croiești eroul."
            }
            HeroChoice::Preset(preset) => preset.flavor(),
        }
    }

    /// The starter items seeded into the persistent loadout on confirm.
    pub fn starter_items(&self) -> &'static [ItemId] {
        match self.choice {
            HeroChoice::Custom => &[],
            HeroChoice::Preset(preset) => preset.starter_items(),
        }
    }

    /// Whether the current path uses curated name cycling.
    pub fn can_cycle_name(&self) -> bool {
        matches!(self.choice, HeroChoice::Custom)
    }

    /// Cycles to the next custom name, wrapping at the end of the list.
    pub fn next_name(&mut self) {
        if !self.can_cycle_name() {
            return;
        }
        self.custom_name_index = (self.custom_name_index + 1) % FOLK_NAMES.len();
    }

    /// Cycles to the previous custom name, wrapping at the start of the list.
    pub fn previous_name(&mut self) {
        if !self.can_cycle_name() {
            return;
        }
        self.custom_name_index = (self.custom_name_index + FOLK_NAMES.len() - 1) % FOLK_NAMES.len();
    }

    /// The attributes as allocated so far.
    pub fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// The appearance as configured so far.
    pub fn appearance(&self) -> PlayerAppearance {
        self.definition.appearance
    }

    /// The stable resolved identity represented by the current preview.
    pub fn definition(&self) -> &CharacterDefinition {
        &self.definition
    }

    pub fn wardrobe(&self) -> WardrobeChoice {
        self.wardrobe
    }

    pub fn part_label(&self, field: CreatorPartField) -> &'static str {
        match field {
            CreatorPartField::Body => option_label(&self.definition.parts.body, &BODY_OPTIONS),
            CreatorPartField::Face => option_label(&self.definition.parts.face, &FACE_OPTIONS),
            CreatorPartField::Hair => option_label(&self.definition.parts.hair, &HAIR_OPTIONS),
            CreatorPartField::Wardrobe => self.wardrobe.label(),
        }
    }

    pub fn cycle_part(
        &mut self,
        field: CreatorPartField,
        direction: CycleDirection,
        catalog: &CharacterCatalog,
    ) -> Result<(), CatalogError> {
        if field == CreatorPartField::Wardrobe {
            let wardrobe = cycle_value(self.wardrobe, &WardrobeChoice::ALL, direction);
            return self.select_wardrobe(wardrobe, catalog);
        }

        let (current, options) = match field {
            CreatorPartField::Body => (&self.definition.parts.body, &BODY_OPTIONS[..]),
            CreatorPartField::Face => (&self.definition.parts.face, &FACE_OPTIONS[..]),
            CreatorPartField::Hair => (&self.definition.parts.hair, &HAIR_OPTIONS[..]),
            CreatorPartField::Wardrobe => unreachable!("wardrobe handled above"),
        };
        let next = cycle_option(current, options, direction);
        let mut candidate = self.definition.clone();
        match field {
            CreatorPartField::Body => {
                candidate.parts.body = catalog_id(catalog, BodyRegion::Body, next.id)?;
            }
            CreatorPartField::Face => {
                candidate.parts.face = catalog_id(catalog, BodyRegion::Face, next.id)?;
            }
            CreatorPartField::Hair => {
                candidate.parts.hair = catalog_id(catalog, BodyRegion::Hair, next.id)?;
            }
            CreatorPartField::Wardrobe => unreachable!("wardrobe handled above"),
        }
        synchronize_appearance_projection(&mut candidate);
        catalog.resolve(&candidate)?;
        self.definition = candidate;
        Ok(())
    }

    pub fn select_wardrobe(
        &mut self,
        wardrobe: WardrobeChoice,
        catalog: &CharacterCatalog,
    ) -> Result<(), CatalogError> {
        let mut candidate = self.definition.clone();
        wardrobe.apply(&mut candidate.parts, catalog)?;
        catalog.resolve(&candidate)?;
        self.definition = candidate;
        self.wardrobe = wardrobe;
        Ok(())
    }

    /// Current value of one attribute.
    pub fn get(&self, kind: AttributeKind) -> u32 {
        self.attributes.get(kind)
    }

    /// Free points spent so far: the spread's total over the per-kind base
    /// values ([`AttributeKind::base_value`]; magie's base is 0).
    pub fn points_spent(&self) -> u32 {
        self.attributes.total() - AttributeKind::base_total()
    }

    /// Free points still available.
    pub fn points_remaining(&self) -> u32 {
        FREE_POINTS - self.points_spent()
    }

    /// Whether any attribute can still be raised (points remain).
    pub fn can_increase(&self) -> bool {
        self.points_remaining() > 0
    }

    /// Whether `kind` can be lowered (it is above its own base value —
    /// magie floors at 0, everything else at 1).
    pub fn can_decrease(&self, kind: AttributeKind) -> bool {
        self.get(kind) > kind.base_value()
    }

    /// Spends one point on `kind`. Returns whether the point was spent.
    pub fn increase(&mut self, kind: AttributeKind) -> bool {
        if !self.can_increase() {
            return false;
        }
        *self.attributes.get_mut(kind) += 1;
        true
    }

    /// Refunds one point from `kind`. Returns whether a point was refunded.
    pub fn decrease(&mut self, kind: AttributeKind) -> bool {
        if !self.can_decrease(kind) {
            return false;
        }
        *self.attributes.get_mut(kind) -= 1;
        true
    }

    pub fn skin_tone(&self) -> SkinTone {
        self.definition.appearance.skin_tone
    }

    pub fn next_skin_tone(&mut self) {
        cycle_next(&mut self.definition.appearance.skin_tone, &SkinTone::ALL);
    }

    pub fn previous_skin_tone(&mut self) {
        cycle_previous(&mut self.definition.appearance.skin_tone, &SkinTone::ALL);
    }

    pub fn accent(&self) -> AccentColor {
        self.definition.appearance.accent
    }

    pub fn next_accent(&mut self) {
        cycle_next(&mut self.definition.appearance.accent, &AccentColor::ALL);
    }

    pub fn previous_accent(&mut self) {
        cycle_previous(&mut self.definition.appearance.accent, &AccentColor::ALL);
    }

    /// Confirm is allowed only when all free points are spent.
    pub fn is_complete(&self) -> bool {
        self.points_remaining() == 0
    }

    /// Restores the fresh custom-draft state.
    pub fn reset(&mut self, catalog: &CharacterCatalog) -> Result<(), CatalogError> {
        *self = Self::default_with_catalog(catalog)?;
        Ok(())
    }
}

fn default_definition(catalog: &CharacterCatalog) -> Result<CharacterDefinition, CatalogError> {
    let mut parts = PartSelections {
        body: catalog_id(catalog, BodyRegion::Body, "human.body.zvelt.v1")?,
        face: catalog_id(catalog, BodyRegion::Face, "human.face.haiduc.v1")?,
        hair: catalog_id(catalog, BodyRegion::Hair, "human.hair.plete.v1")?,
        facial_hair: None,
        torso: catalog_id(catalog, BodyRegion::Torso, "human.torso.ie_altita.v1")?,
        legs: catalog_id(catalog, BodyRegion::Legs, "human.legs.itari.v1")?,
        feet: catalog_id(catalog, BodyRegion::Feet, "human.feet.opinci.v1")?,
        waist: None,
        accessories: Vec::new(),
    };
    WardrobeChoice::Haiduc.apply(&mut parts, catalog)?;
    Ok(CharacterDefinition {
        version: CHARACTER_DEFINITION_VERSION,
        seed: None,
        skeleton: SkeletonFamily::Human,
        culture: CulturalProfile {
            tags: vec!["romanian".to_owned()],
        },
        parts,
        appearance: PlayerAppearance {
            build: BodyBuild::Lean,
            hair: HairStyle::Long,
            ..PlayerAppearance::default()
        },
    })
}

fn definition_for_preset(
    preset: HeroPreset,
    catalog: &CharacterCatalog,
) -> Result<CharacterDefinition, CatalogError> {
    let mut definition = default_definition(catalog)?;
    definition.appearance = preset.appearance();
    let (body, face, hair) = match preset {
        HeroPreset::Haiducul => (
            "human.body.zvelt.v1",
            "human.face.haiduc.v1",
            "human.hair.plete.v1",
        ),
        HeroPreset::Voinicul => (
            "human.body.vanjos.v1",
            "human.face.haiduc.v1",
            "human.hair.scurt.v1",
        ),
        HeroPreset::Ciobanul => (
            "human.body.vanjos.v1",
            "human.face.cioban.v1",
            "human.hair.prins.v1",
        ),
        HeroPreset::UceniculSolomonar => (
            "human.body.zvelt.v1",
            "human.face.cioban.v1",
            "human.hair.plete.v1",
        ),
    };
    definition.parts.body = catalog_id(catalog, BodyRegion::Body, body)?;
    definition.parts.face = catalog_id(catalog, BodyRegion::Face, face)?;
    definition.parts.hair = catalog_id(catalog, BodyRegion::Hair, hair)?;
    wardrobe_for_preset(preset).apply(&mut definition.parts, catalog)?;
    synchronize_appearance_projection(&mut definition);
    catalog.resolve(&definition)?;
    Ok(definition)
}

fn synchronize_appearance_projection(definition: &mut CharacterDefinition) {
    definition.appearance.build = match definition.parts.body.as_str() {
        "human.body.zvelt.v1" => BodyBuild::Lean,
        "human.body.vanjos.v1" => BodyBuild::Sturdy,
        _ => definition.appearance.build,
    };
    definition.appearance.hair = match definition.parts.hair.as_str() {
        "human.hair.plete.v1" => HairStyle::Long,
        "human.hair.prins.v1" => HairStyle::Tied,
        "human.hair.scurt.v1" => HairStyle::Short,
        _ => definition.appearance.hair,
    };
}

const fn wardrobe_for_preset(preset: HeroPreset) -> WardrobeChoice {
    match preset {
        HeroPreset::Haiducul | HeroPreset::Voinicul => WardrobeChoice::Haiduc,
        HeroPreset::Ciobanul | HeroPreset::UceniculSolomonar => WardrobeChoice::Cioban,
    }
}

fn catalog_id(
    catalog: &CharacterCatalog,
    region: BodyRegion,
    value: &'static str,
) -> Result<PartId, CatalogError> {
    catalog
        .compatible_parts(region, SkeletonFamily::Human, &["romanian".to_owned()])
        .into_iter()
        .find(|part| part.id.as_str() == value)
        .map(|part| part.id.clone())
        .ok_or(CatalogError::MissingRequiredRegion(region))
}

fn option_label(current: &PartId, options: &[CreatorPartOption]) -> &'static str {
    options
        .iter()
        .find(|option| option.id == current.as_str())
        .map_or("Necunoscut", |option| option.label)
}

fn cycle_option<'a>(
    current: &PartId,
    options: &'a [CreatorPartOption],
    direction: CycleDirection,
) -> &'a CreatorPartOption {
    let index = options
        .iter()
        .position(|option| option.id == current.as_str())
        .unwrap_or(0);
    let next = match direction {
        CycleDirection::Previous => (index + options.len() - 1) % options.len(),
        CycleDirection::Next => (index + 1) % options.len(),
    };
    &options[next]
}

fn cycle_value<T: Copy + Eq>(value: T, all: &[T], direction: CycleDirection) -> T {
    let index = all
        .iter()
        .position(|candidate| *candidate == value)
        .map_or(0, |index| index);
    match direction {
        CycleDirection::Previous => all[(index + all.len() - 1) % all.len()],
        CycleDirection::Next => all[(index + 1) % all.len()],
    }
}

fn cycle_next<T: Copy + Eq>(value: &mut T, all: &[T]) {
    let index = all
        .iter()
        .position(|candidate| *candidate == *value)
        .expect("current enum value exists in its ALL table");
    *value = all[(index + 1) % all.len()];
}

fn cycle_previous<T: Copy + Eq>(value: &mut T, all: &[T]) {
    let index = all
        .iter()
        .position(|candidate| *candidate == *value)
        .expect("current enum value exists in its ALL table");
    *value = all[(index + all.len() - 1) % all.len()];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> CharacterCatalog {
        CharacterCatalog::from_json(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON")
    }

    #[test]
    fn cycling_face_changes_the_resolved_part_id() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default_with_catalog(&catalog).unwrap();
        let before = draft.definition().parts.face.clone();

        draft
            .cycle_part(CreatorPartField::Face, CycleDirection::Next, &catalog)
            .unwrap();

        assert_ne!(draft.definition().parts.face, before);
        assert!(catalog.resolve(draft.definition()).is_ok());
    }

    fn assert_stable_ids_match_appearance_projection(draft: &CharacterDraft) {
        let expected_build = match draft.definition().parts.body.as_str() {
            "human.body.zvelt.v1" => BodyBuild::Lean,
            "human.body.vanjos.v1" => BodyBuild::Sturdy,
            other => panic!("unexpected creator body ID {other}"),
        };
        let expected_hair = match draft.definition().parts.hair.as_str() {
            "human.hair.plete.v1" => HairStyle::Long,
            "human.hair.prins.v1" => HairStyle::Tied,
            "human.hair.scurt.v1" => HairStyle::Short,
            other => panic!("unexpected creator hair ID {other}"),
        };
        assert_eq!(draft.appearance().build, expected_build);
        assert_eq!(draft.appearance().hair, expected_hair);
    }

    #[test]
    fn default_and_ucenicul_ids_match_the_legacy_appearance_projection() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default_with_catalog(&catalog).unwrap();
        assert_stable_ids_match_appearance_projection(&draft);

        draft
            .select_choice(HeroChoice::Preset(HeroPreset::UceniculSolomonar), &catalog)
            .unwrap();
        assert_stable_ids_match_appearance_projection(&draft);
    }

    #[test]
    fn every_body_and_hair_cycle_keeps_the_appearance_projection_in_sync() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default_with_catalog(&catalog).unwrap();

        for _ in 0..BODY_OPTIONS.len() {
            assert_stable_ids_match_appearance_projection(&draft);
            draft
                .cycle_part(CreatorPartField::Body, CycleDirection::Next, &catalog)
                .unwrap();
        }
        for _ in 0..HAIR_OPTIONS.len() {
            assert_stable_ids_match_appearance_projection(&draft);
            draft
                .cycle_part(CreatorPartField::Hair, CycleDirection::Next, &catalog)
                .unwrap();
        }
    }

    #[test]
    fn cioban_wardrobe_changes_a_complete_compatible_set() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default_with_catalog(&catalog).unwrap();

        draft
            .select_wardrobe(WardrobeChoice::Cioban, &catalog)
            .unwrap();

        assert_eq!(
            draft.definition().parts.torso.as_str(),
            "human.torso.camasa_ciobaneasca.v1"
        );
        assert_eq!(
            draft.definition().parts.legs.as_str(),
            "human.legs.cioareci.v1"
        );
        assert_eq!(
            draft.definition().parts.feet.as_str(),
            "human.feet.opinci.v1"
        );
        assert_eq!(draft.wardrobe(), WardrobeChoice::Cioban);
        assert!(catalog.resolve(draft.definition()).is_ok());
    }

    #[test]
    fn preset_and_reset_keep_catalog_backed_definition_ids() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default_with_catalog(&catalog).unwrap();

        draft
            .select_choice(HeroChoice::Preset(HeroPreset::Ciobanul), &catalog)
            .unwrap();
        assert_eq!(
            draft.definition().parts.body.as_str(),
            "human.body.vanjos.v1"
        );
        assert_eq!(
            draft.definition().parts.face.as_str(),
            "human.face.cioban.v1"
        );
        assert_eq!(
            draft.definition().parts.hair.as_str(),
            "human.hair.prins.v1"
        );
        assert!(catalog.resolve(draft.definition()).is_ok());

        draft.reset(&catalog).unwrap();
        assert_eq!(
            draft.definition().parts.body.as_str(),
            "human.body.zvelt.v1"
        );
        assert_eq!(
            draft.definition().parts.face.as_str(),
            "human.face.haiduc.v1"
        );
        assert_eq!(
            draft.definition().parts.hair.as_str(),
            "human.hair.plete.v1"
        );
        assert!(catalog.resolve(draft.definition()).is_ok());
    }

    #[test]
    fn fresh_draft_is_custom_with_base_attributes_and_default_appearance() {
        let draft = CharacterDraft::default();
        assert_eq!(draft.choice(), HeroChoice::Custom);
        assert_eq!(draft.attributes(), Attributes::default());
        assert_eq!(
            draft.appearance(),
            PlayerAppearance {
                build: BodyBuild::Lean,
                hair: HairStyle::Long,
                ..PlayerAppearance::default()
            }
        );
        assert_eq!(draft.points_spent(), 0);
        assert_eq!(draft.points_remaining(), FREE_POINTS);
        assert_eq!(draft.name(), FOLK_NAMES[0]);
        assert!(draft.can_cycle_name());
        assert!(!draft.is_complete());
        assert!(draft.starter_items().is_empty());
    }

    #[test]
    fn presets_have_unique_names_and_equal_point_budgets() {
        let mut names = std::collections::BTreeSet::new();
        for preset in HeroPreset::ALL {
            assert!(names.insert(preset.name()), "preset names stay unique");
            let attrs = preset.attributes();
            assert_eq!(
                attrs.total(),
                AttributeKind::base_total() + FREE_POINTS,
                "{} uses the same point budget as custom heroes",
                preset.name()
            );
            for kind in AttributeKind::ALL {
                assert!(
                    attrs.get(kind) >= kind.base_value(),
                    "{}'s {kind:?} sits at or above its base",
                    preset.name()
                );
            }
        }
    }

    /// `magie == 0` is a valid, buildable non-caster: preset spreads may
    /// carry it and the draft floors magie at 0, never normalizing upward.
    #[test]
    fn a_magie_zero_preset_is_a_valid_non_caster() {
        let catalog = catalog();
        let attrs = HeroPreset::Voinicul.attributes();
        assert_eq!(attrs.magie, 0);
        assert_eq!(crate::character::stats::max_mana(&attrs), 0);
        let mut draft = CharacterDraft::default();
        draft
            .select_choice(HeroChoice::Preset(HeroPreset::Voinicul), &catalog)
            .unwrap();
        assert!(
            !draft.can_decrease(AttributeKind::Magie),
            "magie floors at 0"
        );
        assert!(draft.is_complete(), "magie 0 still counts as fully spent");
    }

    #[test]
    fn selecting_a_preset_populates_name_attributes_appearance_and_loadout() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default();
        draft
            .select_choice(HeroChoice::Preset(HeroPreset::Ciobanul), &catalog)
            .unwrap();
        assert_eq!(draft.choice(), HeroChoice::Preset(HeroPreset::Ciobanul));
        assert_eq!(draft.name(), "Ciobanul");
        assert_eq!(draft.attributes(), HeroPreset::Ciobanul.attributes());
        assert_eq!(draft.appearance(), HeroPreset::Ciobanul.appearance());
        assert_eq!(draft.starter_items(), HeroPreset::Ciobanul.starter_items());
        assert!(
            draft.is_complete(),
            "presets start as fully allocated builds"
        );
        assert!(!draft.can_cycle_name(), "preset names stay fixed");
    }

    #[test]
    fn switching_back_to_custom_restores_the_custom_baseline() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default();
        draft
            .select_choice(HeroChoice::Preset(HeroPreset::Voinicul), &catalog)
            .unwrap();
        draft.select_choice(HeroChoice::Custom, &catalog).unwrap();
        assert_eq!(draft.choice(), HeroChoice::Custom);
        assert_eq!(draft.attributes(), Attributes::default());
        assert_eq!(
            draft.appearance(),
            PlayerAppearance {
                build: BodyBuild::Lean,
                hair: HairStyle::Long,
                ..PlayerAppearance::default()
            }
        );
        assert_eq!(draft.name(), FOLK_NAMES[0]);
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }

    #[test]
    fn increase_spends_a_point() {
        let mut draft = CharacterDraft::default();
        assert!(draft.increase(AttributeKind::Putere));
        assert_eq!(draft.get(AttributeKind::Putere), 2);
        assert_eq!(draft.points_remaining(), FREE_POINTS - 1);
    }

    #[test]
    fn cannot_overspend_past_free_points() {
        let mut draft = CharacterDraft::default();
        for _ in 0..FREE_POINTS {
            assert!(draft.increase(AttributeKind::Agilitate));
        }
        assert_eq!(draft.points_remaining(), 0);
        assert!(!draft.can_increase());
        assert!(!draft.increase(AttributeKind::Putere), "no points left");
        assert_eq!(
            draft.get(AttributeKind::Putere),
            AttributeKind::Putere.base_value()
        );
        assert_eq!(
            draft.get(AttributeKind::Agilitate),
            AttributeKind::Agilitate.base_value() + FREE_POINTS
        );
    }

    #[test]
    fn cannot_drop_below_each_kinds_base_value() {
        let mut draft = CharacterDraft::default();
        for kind in AttributeKind::ALL {
            assert!(!draft.can_decrease(kind), "{kind:?} sits at its base");
            assert!(!draft.decrease(kind));
            assert_eq!(draft.get(kind), kind.base_value());
        }
        assert_eq!(draft.points_remaining(), FREE_POINTS, "nothing refunded");
    }

    #[test]
    fn magie_can_be_bought_up_from_zero_and_refunded_back_to_zero() {
        let mut draft = CharacterDraft::default();
        assert_eq!(draft.get(AttributeKind::Magie), 0, "custom base is 0");
        assert!(draft.increase(AttributeKind::Magie));
        assert_eq!(draft.get(AttributeKind::Magie), 1);
        assert!(draft.can_decrease(AttributeKind::Magie));
        assert!(draft.decrease(AttributeKind::Magie));
        assert_eq!(draft.get(AttributeKind::Magie), 0);
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }

    #[test]
    fn decrease_refunds_a_point() {
        let mut draft = CharacterDraft::default();
        draft.increase(AttributeKind::Noroc);
        assert!(draft.can_decrease(AttributeKind::Noroc));
        assert!(draft.decrease(AttributeKind::Noroc));
        assert_eq!(
            draft.get(AttributeKind::Noroc),
            AttributeKind::Noroc.base_value()
        );
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }

    #[test]
    fn complete_only_when_exactly_all_points_spent() {
        // 16 free points over 8 kinds: one point on each kind twice spends
        // them all; every intermediate state is incomplete.
        let mut draft = CharacterDraft::default();
        for kind in AttributeKind::ALL {
            draft.increase(kind);
        }
        assert!(!draft.is_complete(), "8 of 16 spent");
        for kind in AttributeKind::ALL {
            assert!(!draft.is_complete());
            draft.increase(kind);
        }
        assert!(draft.is_complete(), "all 16 spent");
        draft.decrease(AttributeKind::Putere);
        assert!(!draft.is_complete(), "refund drops completeness");
    }

    #[test]
    fn palette_cycles_wrap_for_every_field() {
        let mut draft = CharacterDraft::default();

        draft.previous_skin_tone();
        assert_eq!(draft.skin_tone(), SkinTone::Fair);
        draft.next_skin_tone();
        assert_eq!(draft.skin_tone(), SkinTone::Warm);

        draft.previous_accent();
        assert_eq!(draft.accent(), AccentColor::Storm);
        draft.next_accent();
        assert_eq!(draft.accent(), AccentColor::Crimson);
    }

    #[test]
    fn custom_name_cycles_forward_and_wraps() {
        let mut draft = CharacterDraft::default();
        for expected in FOLK_NAMES.iter().skip(1) {
            draft.next_name();
            assert_eq!(draft.name(), *expected);
        }
        draft.next_name();
        assert_eq!(draft.name(), FOLK_NAMES[0], "wraps to the first name");
    }

    #[test]
    fn custom_name_cycles_backward_and_wraps() {
        let mut draft = CharacterDraft::default();
        draft.previous_name();
        assert_eq!(
            draft.name(),
            FOLK_NAMES[FOLK_NAMES.len() - 1],
            "wraps to the last name"
        );
        draft.previous_name();
        assert_eq!(draft.name(), FOLK_NAMES[FOLK_NAMES.len() - 2]);
    }

    #[test]
    fn reset_restores_the_fresh_custom_draft() {
        let catalog = catalog();
        let mut draft = CharacterDraft::default();
        draft
            .select_choice(HeroChoice::Preset(HeroPreset::Haiducul), &catalog)
            .unwrap();
        draft.previous_accent();
        draft.decrease(AttributeKind::Noroc);
        draft.reset(&catalog).unwrap();
        assert_eq!(draft, CharacterDraft::default());
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }
}
