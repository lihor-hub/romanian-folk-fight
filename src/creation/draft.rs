//! Pure hero-creation rules for the character creation screen.
//!
//! No ECS systems here: [`CharacterDraft`] is a plain value type (registered
//! as a `Resource` by the plugin) so the allocation invariants, preset data,
//! and appearance cycling remain unit-testable without a `World`.

use bevy::prelude::*;

use crate::character::{
    AccentColor, Attributes, BodyBuild, CharacterDefinition, HairStyle, PlayerAppearance, SkinTone,
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

    /// The stable resolved identity represented by the current preview.
    ///
    /// The first modular player tracer bullet maps the existing appearance
    /// controls onto the versioned human definition. Later catalog-backed
    /// selectors can replace this adapter without changing confirmation or
    /// persistence consumers.
    pub fn definition(&self) -> CharacterDefinition {
        CharacterDefinition::legacy_human(self.appearance)
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
        let attrs = HeroPreset::Voinicul.attributes();
        assert_eq!(attrs.magie, 0);
        assert_eq!(crate::character::stats::max_mana(&attrs), 0);
        let mut draft = CharacterDraft::default();
        draft.select_choice(HeroChoice::Preset(HeroPreset::Voinicul));
        assert!(
            !draft.can_decrease(AttributeKind::Magie),
            "magie floors at 0"
        );
        assert!(draft.is_complete(), "magie 0 still counts as fully spent");
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
}
