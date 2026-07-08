//! Pure hero-creation rules for the character creation screen.
//!
//! No ECS systems here: [`CharacterDraft`] is a plain value type (registered
//! as a `Resource` by the plugin) so the allocation invariants, preset data,
//! and appearance cycling remain unit-testable without a `World`.

use bevy::prelude::*;

use crate::character::{
    AccentColor, Attributes, BodyBuild, CostumeStyle, HairStyle, HairVariant, HeadFeature,
    PlayerAppearance, SkinTone,
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

/// Free attribute points to distribute on top of the base values.
pub const FREE_POINTS: u32 = 10;

/// Every attribute starts at this value and can never drop below it.
pub const BASE_VALUE: u32 = 1;

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

    pub fn attributes(self) -> Attributes {
        match self {
            Self::Haiducul => Attributes {
                putere: 2,
                agilitate: 5,
                vitalitate: 2,
                noroc: 5,
            },
            Self::Voinicul => Attributes {
                putere: 4,
                agilitate: 3,
                vitalitate: 4,
                noroc: 3,
            },
            Self::Ciobanul => Attributes {
                putere: 3,
                agilitate: 2,
                vitalitate: 6,
                noroc: 3,
            },
            Self::UceniculSolomonar => Attributes {
                putere: 2,
                agilitate: 2,
                vitalitate: 5,
                noroc: 5,
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
                costume: CostumeStyle::HaiducCoat,
                head_feature: HeadFeature::Moustache,
                hair_variant: HairVariant::Primary,
            },
            Self::Voinicul => PlayerAppearance {
                skin_tone: SkinTone::Warm,
                build: BodyBuild::Powerful,
                hair: HairStyle::Short,
                accent: AccentColor::Crimson,
                costume: CostumeStyle::VoinicTunic,
                head_feature: HeadFeature::Clean,
                hair_variant: HairVariant::Primary,
            },
            Self::Ciobanul => PlayerAppearance {
                skin_tone: SkinTone::Fair,
                build: BodyBuild::Sturdy,
                hair: HairStyle::Tied,
                accent: AccentColor::Gold,
                costume: CostumeStyle::CiobanCojoc,
                head_feature: HeadFeature::Beard,
                hair_variant: HairVariant::Alternate,
            },
            Self::UceniculSolomonar => PlayerAppearance {
                skin_tone: SkinTone::Deep,
                build: BodyBuild::Balanced,
                hair: HairStyle::Braided,
                accent: AccentColor::Storm,
                costume: CostumeStyle::SolomonarRobe,
                head_feature: HeadFeature::Beard,
                hair_variant: HairVariant::Ornate,
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
    appearance: PlayerAppearance,
}

impl Default for CharacterDraft {
    fn default() -> Self {
        Self {
            choice: HeroChoice::Custom,
            custom_name_index: 0,
            attributes: Attributes::default(),
            appearance: PlayerAppearance::default(),
        }
    }
}

impl CharacterDraft {
    /// The currently selected hero choice.
    pub fn choice(&self) -> HeroChoice {
        self.choice
    }

    /// Apply one starter choice, resetting stats and appearance to that
    /// template while keeping the draft editable afterward.
    pub fn select_choice(&mut self, choice: HeroChoice) {
        self.choice = choice;
        match choice {
            HeroChoice::Custom => {
                self.attributes = Attributes::default();
                self.appearance = PlayerAppearance::default();
            }
            HeroChoice::Preset(preset) => {
                self.attributes = preset.attributes();
                self.appearance = preset.appearance();
            }
        }
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
        self.appearance
    }

    /// Current value of one attribute.
    pub fn get(&self, kind: AttributeKind) -> u32 {
        self.attributes.get(kind)
    }

    /// Free points spent so far.
    pub fn points_spent(&self) -> u32 {
        let attrs = &self.attributes;
        (attrs.putere + attrs.agilitate + attrs.vitalitate + attrs.noroc)
            - AttributeKind::ALL.len() as u32 * BASE_VALUE
    }

    /// Free points still available.
    pub fn points_remaining(&self) -> u32 {
        FREE_POINTS - self.points_spent()
    }

    /// Whether any attribute can still be raised (points remain).
    pub fn can_increase(&self) -> bool {
        self.points_remaining() > 0
    }

    /// Whether `kind` can be lowered (it is above the base value).
    pub fn can_decrease(&self, kind: AttributeKind) -> bool {
        self.get(kind) > BASE_VALUE
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
        self.appearance.skin_tone
    }

    pub fn next_skin_tone(&mut self) {
        cycle_next(&mut self.appearance.skin_tone, &SkinTone::ALL);
    }

    pub fn previous_skin_tone(&mut self) {
        cycle_previous(&mut self.appearance.skin_tone, &SkinTone::ALL);
    }

    pub fn build(&self) -> BodyBuild {
        self.appearance.build
    }

    pub fn next_build(&mut self) {
        cycle_next(&mut self.appearance.build, &BodyBuild::ALL);
    }

    pub fn previous_build(&mut self) {
        cycle_previous(&mut self.appearance.build, &BodyBuild::ALL);
    }

    pub fn hair(&self) -> HairStyle {
        self.appearance.hair
    }

    pub fn next_hair(&mut self) {
        cycle_next(&mut self.appearance.hair, &HairStyle::ALL);
    }

    pub fn previous_hair(&mut self) {
        cycle_previous(&mut self.appearance.hair, &HairStyle::ALL);
    }

    pub fn accent(&self) -> AccentColor {
        self.appearance.accent
    }

    pub fn next_accent(&mut self) {
        cycle_next(&mut self.appearance.accent, &AccentColor::ALL);
    }

    pub fn previous_accent(&mut self) {
        cycle_previous(&mut self.appearance.accent, &AccentColor::ALL);
    }

    pub fn costume(&self) -> CostumeStyle {
        self.appearance.costume
    }

    pub fn next_costume(&mut self) {
        cycle_next(&mut self.appearance.costume, &CostumeStyle::ALL);
    }

    pub fn previous_costume(&mut self) {
        cycle_previous(&mut self.appearance.costume, &CostumeStyle::ALL);
    }

    pub fn head_feature(&self) -> HeadFeature {
        self.appearance.head_feature
    }

    pub fn next_head_feature(&mut self) {
        cycle_next(&mut self.appearance.head_feature, &HeadFeature::ALL);
    }

    pub fn previous_head_feature(&mut self) {
        cycle_previous(&mut self.appearance.head_feature, &HeadFeature::ALL);
    }

    pub fn hair_variant(&self) -> HairVariant {
        self.appearance.hair_variant
    }

    pub fn next_hair_variant(&mut self) {
        cycle_next(&mut self.appearance.hair_variant, &HairVariant::ALL);
    }

    pub fn previous_hair_variant(&mut self) {
        cycle_previous(&mut self.appearance.hair_variant, &HairVariant::ALL);
    }

    /// Confirm is allowed only when all free points are spent.
    pub fn is_complete(&self) -> bool {
        self.points_remaining() == 0
    }

    /// Restores the fresh custom-draft state.
    pub fn reset(&mut self) {
        *self = Self::default();
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

    #[test]
    fn fresh_draft_is_custom_with_base_attributes_and_default_appearance() {
        let draft = CharacterDraft::default();
        assert_eq!(draft.choice(), HeroChoice::Custom);
        assert_eq!(draft.attributes(), Attributes::default());
        assert_eq!(draft.appearance(), PlayerAppearance::default());
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
            let points = attrs.putere + attrs.agilitate + attrs.vitalitate + attrs.noroc;
            assert_eq!(
                points,
                AttributeKind::ALL.len() as u32 * BASE_VALUE + FREE_POINTS,
                "{} uses the same point budget as custom heroes",
                preset.name()
            );
        }
    }

    #[test]
    fn selecting_a_preset_populates_name_attributes_appearance_and_loadout() {
        let mut draft = CharacterDraft::default();
        draft.select_choice(HeroChoice::Preset(HeroPreset::Ciobanul));
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
        let mut draft = CharacterDraft::default();
        draft.select_choice(HeroChoice::Preset(HeroPreset::Voinicul));
        draft.select_choice(HeroChoice::Custom);
        assert_eq!(draft.choice(), HeroChoice::Custom);
        assert_eq!(draft.attributes(), Attributes::default());
        assert_eq!(draft.appearance(), PlayerAppearance::default());
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
        assert_eq!(draft.get(AttributeKind::Putere), BASE_VALUE);
        assert_eq!(
            draft.get(AttributeKind::Agilitate),
            BASE_VALUE + FREE_POINTS
        );
    }

    #[test]
    fn cannot_drop_below_base_value() {
        let mut draft = CharacterDraft::default();
        assert!(!draft.can_decrease(AttributeKind::Vitalitate));
        assert!(!draft.decrease(AttributeKind::Vitalitate));
        assert_eq!(draft.get(AttributeKind::Vitalitate), BASE_VALUE);
        assert_eq!(draft.points_remaining(), FREE_POINTS, "nothing refunded");
    }

    #[test]
    fn decrease_refunds_a_point() {
        let mut draft = CharacterDraft::default();
        draft.increase(AttributeKind::Noroc);
        assert!(draft.can_decrease(AttributeKind::Noroc));
        assert!(draft.decrease(AttributeKind::Noroc));
        assert_eq!(draft.get(AttributeKind::Noroc), BASE_VALUE);
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }

    #[test]
    fn complete_only_when_exactly_all_points_spent() {
        let mut draft = CharacterDraft::default();
        for kind in AttributeKind::ALL {
            draft.increase(kind);
            draft.increase(kind);
        }
        assert!(!draft.is_complete());
        draft.increase(AttributeKind::Putere);
        assert!(!draft.is_complete(), "9 of 10 spent");
        draft.increase(AttributeKind::Vitalitate);
        assert!(draft.is_complete(), "all 10 spent");
        draft.decrease(AttributeKind::Putere);
        assert!(!draft.is_complete(), "refund drops completeness");
    }

    #[test]
    fn appearance_cycles_wrap_for_every_field() {
        let mut draft = CharacterDraft::default();

        draft.previous_skin_tone();
        assert_eq!(draft.skin_tone(), SkinTone::Fair);
        draft.next_skin_tone();
        assert_eq!(draft.skin_tone(), SkinTone::Warm);

        draft.previous_build();
        assert_eq!(draft.build(), BodyBuild::Lean);
        draft.next_build();
        assert_eq!(draft.build(), BodyBuild::Balanced);

        draft.previous_hair();
        assert_eq!(draft.hair(), HairStyle::Tied);
        draft.next_hair();
        assert_eq!(draft.hair(), HairStyle::Braided);

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
        let mut draft = CharacterDraft::default();
        draft.select_choice(HeroChoice::Preset(HeroPreset::Haiducul));
        draft.previous_accent();
        draft.decrease(AttributeKind::Noroc);
        draft.reset();
        assert_eq!(draft, CharacterDraft::default());
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }

    #[test]
    fn each_preset_resolves_to_its_authored_appearance_bundle() {
        for (preset, expected) in [
            (
                HeroPreset::Haiducul,
                PlayerAppearance {
                    skin_tone: SkinTone::Olive,
                    build: BodyBuild::Lean,
                    hair: HairStyle::Long,
                    accent: AccentColor::Forest,
                    costume: CostumeStyle::HaiducCoat,
                    head_feature: HeadFeature::Moustache,
                    hair_variant: HairVariant::Primary,
                },
            ),
            (
                HeroPreset::Voinicul,
                PlayerAppearance {
                    skin_tone: SkinTone::Warm,
                    build: BodyBuild::Powerful,
                    hair: HairStyle::Short,
                    accent: AccentColor::Crimson,
                    costume: CostumeStyle::VoinicTunic,
                    head_feature: HeadFeature::Clean,
                    hair_variant: HairVariant::Primary,
                },
            ),
            (
                HeroPreset::Ciobanul,
                PlayerAppearance {
                    skin_tone: SkinTone::Fair,
                    build: BodyBuild::Sturdy,
                    hair: HairStyle::Tied,
                    accent: AccentColor::Gold,
                    costume: CostumeStyle::CiobanCojoc,
                    head_feature: HeadFeature::Beard,
                    hair_variant: HairVariant::Alternate,
                },
            ),
            (
                HeroPreset::UceniculSolomonar,
                PlayerAppearance {
                    skin_tone: SkinTone::Deep,
                    build: BodyBuild::Balanced,
                    hair: HairStyle::Braided,
                    accent: AccentColor::Storm,
                    costume: CostumeStyle::SolomonarRobe,
                    head_feature: HeadFeature::Beard,
                    hair_variant: HairVariant::Ornate,
                },
            ),
        ] {
            assert_eq!(
                preset.appearance(),
                expected,
                "{} keeps its curated appearance bundle",
                preset.name()
            );
        }
    }

    #[test]
    fn preset_appearances_are_pairwise_distinct_across_every_field() {
        let bundles: Vec<PlayerAppearance> =
            HeroPreset::ALL.iter().map(|p| p.appearance()).collect();
        for i in 0..bundles.len() {
            for j in (i + 1)..bundles.len() {
                let (a, b) = (bundles[i], bundles[j]);
                assert_ne!(
                    a.skin_tone,
                    b.skin_tone,
                    "presets {} and {} share a skin tone",
                    HeroPreset::ALL[i].name(),
                    HeroPreset::ALL[j].name()
                );
                assert_ne!(
                    a.build,
                    b.build,
                    "presets {} and {} share a build",
                    HeroPreset::ALL[i].name(),
                    HeroPreset::ALL[j].name()
                );
                assert_ne!(
                    a.hair,
                    b.hair,
                    "presets {} and {} share a hair style",
                    HeroPreset::ALL[i].name(),
                    HeroPreset::ALL[j].name()
                );
                assert_ne!(
                    a.accent,
                    b.accent,
                    "presets {} and {} share an accent color",
                    HeroPreset::ALL[i].name(),
                    HeroPreset::ALL[j].name()
                );
            }
        }
    }

    #[test]
    fn selecting_each_preset_repopulates_the_draft_appearance() {
        for preset in HeroPreset::ALL {
            let mut draft = CharacterDraft::default();
            draft.select_choice(HeroChoice::Preset(preset));
            assert_eq!(
                draft.appearance(),
                preset.appearance(),
                "{} draft appearance matches the preset bundle",
                preset.name()
            );
            assert_eq!(draft.attributes(), preset.attributes());
            assert_eq!(draft.name(), preset.name());
        }
    }

    #[test]
    fn default_appearance_seeds_new_taxonomy_fields() {
        let appearance = PlayerAppearance::default();
        assert_eq!(appearance.costume, CostumeStyle::default());
        assert_eq!(appearance.head_feature, HeadFeature::default());
        assert_eq!(appearance.hair_variant, HairVariant::default());
    }

    #[test]
    fn each_preset_binds_a_distinct_costume_style() {
        let costumes: Vec<CostumeStyle> = HeroPreset::ALL
            .iter()
            .map(|p| p.appearance().costume)
            .collect();
        for i in 0..costumes.len() {
            for j in (i + 1)..costumes.len() {
                assert_ne!(
                    costumes[i],
                    costumes[j],
                    "presets {} and {} share a costume style",
                    HeroPreset::ALL[i].name(),
                    HeroPreset::ALL[j].name()
                );
            }
        }
    }

    #[test]
    fn preset_head_feature_and_hair_variant_are_authored() {
        for (preset, expected_feature, expected_variant) in [
            (
                HeroPreset::Haiducul,
                HeadFeature::Moustache,
                HairVariant::Primary,
            ),
            (
                HeroPreset::Voinicul,
                HeadFeature::Clean,
                HairVariant::Primary,
            ),
            (
                HeroPreset::Ciobanul,
                HeadFeature::Beard,
                HairVariant::Alternate,
            ),
            (
                HeroPreset::UceniculSolomonar,
                HeadFeature::Beard,
                HairVariant::Ornate,
            ),
        ] {
            let bundle = preset.appearance();
            assert_eq!(
                bundle.head_feature,
                expected_feature,
                "{} keeps its authored head feature",
                preset.name()
            );
            assert_eq!(
                bundle.hair_variant,
                expected_variant,
                "{} keeps its authored hair variant",
                preset.name()
            );
        }
    }

    #[test]
    fn appearance_cycles_wrap_for_new_taxonomy_fields() {
        let mut draft = CharacterDraft::default();

        draft.previous_costume();
        assert_eq!(draft.costume(), CostumeStyle::SolomonarRobe);
        draft.next_costume();
        assert_eq!(draft.costume(), CostumeStyle::HaiducCoat);

        draft.previous_head_feature();
        assert_eq!(draft.head_feature(), HeadFeature::Beard);
        draft.next_head_feature();
        assert_eq!(draft.head_feature(), HeadFeature::Clean);

        draft.previous_hair_variant();
        assert_eq!(draft.hair_variant(), HairVariant::Ornate);
        draft.next_hair_variant();
        assert_eq!(draft.hair_variant(), HairVariant::Primary);
    }

    #[test]
    fn switching_back_to_custom_restores_new_taxonomy_defaults() {
        let mut draft = CharacterDraft::default();
        draft.select_choice(HeroChoice::Preset(HeroPreset::Ciobanul));
        assert_ne!(draft.costume(), CostumeStyle::default());
        draft.select_choice(HeroChoice::Custom);
        assert_eq!(draft.costume(), CostumeStyle::default());
        assert_eq!(draft.head_feature(), HeadFeature::default());
        assert_eq!(draft.hair_variant(), HairVariant::default());
    }
}
