//! The folklore opponent ladder (#20): ten Romanian folklore creatures of
//! escalating difficulty, with a boss every five fights, as plain static
//! data. The arena spawns the current [`LadderProgress`] opponent, the
//! progression flow advances the ladder on victory, and after opponent 10
//! the ladder loops with every attribute total raised by 20% per lap
//! ("Turul 2", "Turul 3", ...).
//!
//! Attribute budgets are enforced by tests: a non-boss opponent of level `L`
//! carries `7 + 5 * L` total attribute points, a boss `7 + 6 * L`. The
//! spreads themselves are data, tuned per creature flavor. #128 widened the
//! budgets from the four-attribute era (`4 + 3 * L` / `4 + 4 * L`) alongside
//! the player's own pools (`creation::FREE_POINTS`,
//! `progression::POINTS_PER_LEVEL`), keeping the opponent-to-player total
//! ratio close to its pre-#128 curve; #149 may retune.

use std::sync::OnceLock;

use bevy::prelude::*;

use crate::character::{
    AttributeKind, Attributes, BodyRegion, CatalogError, CharacterCatalog, CharacterDefinition,
    CulturalProfile, GenerationError, GenerationProfile, GenerationSlot, HairStyle, PartId,
    PlayerAppearance, SkeletonFamily, WeightedPart, generate_character,
};
use crate::cutout::CutoutTemplate;
use crate::items::ItemId;

/// Base attribute points every fighter starts from — the sum of the eight
/// per-kind base values ([`AttributeKind::base_total`]: seven 1s plus magie
/// 0), pinned by a test below.
const BUDGET_BASE: u32 = 7;
/// Attribute points per level for a non-boss opponent.
const POINTS_PER_LEVEL: u32 = 5;
/// Attribute points per level for a boss.
const BOSS_POINTS_PER_LEVEL: u32 = 6;
/// Extra attribute-total percent per completed lap of the ladder.
const LAP_BONUS_PERCENT: u32 = 20;

/// The reproducible campaign seed used by ordinary runs. Review builds may
/// replace it before entering an encounter without changing production save
/// or combat-RNG behavior.
pub const DEFAULT_CAMPAIGN_SEED: u64 = 0;

/// Stable authored identity of the first generated-human tracer bullet.
/// This deliberately does not reuse the display name: copy changes and the
/// Romanian lap suffix must not silently reroll the fighter.
pub const HOT_DE_CODRU_ENCOUNTER_ID: &str = "ladder.hot_de_codru.v1";

/// Seed shared by every encounter in one campaign. The first generated-human
/// slice derives its own seed from this value plus its authored encounter ID.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CampaignSeed(pub u64);

impl Default for CampaignSeed {
    fn default() -> Self {
        Self(DEFAULT_CAMPAIGN_SEED)
    }
}

/// Exact generated identity attached to the representative human opponent.
/// The resolved stable IDs remain authoritative in `definition`; `seed`
/// records the deterministic provenance used to select the unlocked parts.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct SeededOpponent {
    pub encounter_id: &'static str,
    pub seed: u64,
    pub definition: CharacterDefinition,
}

/// One rung of the opponent ladder: pure static data the arena turns into a
/// fighter entity (attributes, AI profile, equipment) and the announcer and
/// progression read for flavor and rewards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opponent {
    /// Display name, also used for the arena label.
    pub name: &'static str,
    /// Enemy level fed into the reward and XP formulas.
    pub level: u32,
    /// Base attribute spread; laps past the first scale it (see
    /// [`scaled_attributes`]).
    pub attrs: Attributes,
    /// The `AiProfile` aggression this opponent fights with.
    pub aggression: f32,
    /// Catalog items the opponent spawns equipped with.
    pub equipment: &'static [ItemId],
    /// Which runtime cutout skeleton the arena should render for this entry.
    pub cutout_template: CutoutTemplate,
    /// Bosses get a distinct label color, an announcer intro, and double XP.
    pub is_boss: bool,
    /// The announcer's intro line; only shown for bosses.
    pub intro_line: &'static str,
    /// This opponent's one muted accent hue (#118): a `Sprite::color` tint
    /// the arena applies to the clothing/accent parts of its cutout rig
    /// (never skin, outline, or the player's own untinted template) so every
    /// ladder entry reads as visually distinct on top of the shared folk
    /// palette. See `docs/art-direction.md`.
    pub accent_hue: Color,
}

/// Shorthand for a ladder entry; keeps the [`LADDER`] table readable. The
/// spread comes in as a named-field [`Attributes`] literal (#128: positional
/// numbers stopped being readable at eight attributes).
#[allow(clippy::too_many_arguments)]
const fn opponent(
    name: &'static str,
    level: u32,
    attrs: Attributes,
    aggression: f32,
    equipment: &'static [ItemId],
    cutout_template: CutoutTemplate,
    is_boss: bool,
    intro_line: &'static str,
    accent_hue: Color,
) -> Opponent {
    Opponent {
        name,
        level,
        attrs,
        aggression,
        equipment,
        cutout_template,
        is_boss,
        intro_line,
        accent_hue,
    }
}

/// One muted accent hue per ladder entry (#118), applied as a `Sprite::color`
/// tint on the clothing/accent parts of that opponent's cutout rig only
/// (never skin, outline, or the player's own untinted template). Kept light
/// enough to multiply over the shared folk-palette artwork as a wash rather
/// than a full recolor -- see `docs/art-direction.md`'s "one muted accent
/// hue... on top of this shared base". Consecutive ladder entries are never
/// assigned the same hue (in fact every entry below is pairwise distinct).
const HOT_DE_CODRU_HUE: Color = Color::srgb(0.55, 0.68, 0.50); // mossy forest green: a woodland bandit
const STRIGOI_HUE: Color = Color::srgb(0.70, 0.68, 0.66); // pale ash gray: risen from the grave
const VARCOLAC_HUE: Color = Color::srgb(0.55, 0.60, 0.68); // storm slate-blue: a night predator
const CAPCAUN_HUE: Color = Color::srgb(0.72, 0.58, 0.42); // muddy ochre/clay: a hungry ogre
const MUMA_PADURII_HUE: Color = Color::srgb(0.50, 0.56, 0.38); // deep moss/olive: the forest mother
const IELE_HUE: Color = Color::srgb(0.68, 0.60, 0.74); // pale lavender mist: the fae dancers
const SOLOMONAR_HUE: Color = Color::srgb(0.48, 0.56, 0.66); // storm-cloud blue: the weather wizard
const BALAUR_HUE: Color = Color::srgb(0.42, 0.64, 0.60); // verdigris teal: the three-headed dragon
const ZMEU_HUE: Color = Color::srgb(0.74, 0.50, 0.34); // ember copper: the dragon boss
const ZMEUL_ZMEILOR_HUE: Color = Color::srgb(0.58, 0.42, 0.60); // muted royal plum: the dragon king

/// The full ladder, in fight order. Budgets, monotonic levels, and the boss
/// positions are pinned by the integrity tests below.
pub static LADDER: [Opponent; 10] = [
    opponent(
        "Hoț de codru",
        1,
        Attributes {
            putere: 2,
            agilitate: 2,
            vitalitate: 2,
            noroc: 1,
            atac: 2,
            aparare: 1,
            carisma: 1,
            magie: 1,
        },
        0.25,
        &[],
        CutoutTemplate::Human,
        false,
        "Un hoț de codru pândește punga voinicului. Păzea!",
        HOT_DE_CODRU_HUE,
    ),
    opponent(
        "Strigoi",
        2,
        Attributes {
            putere: 2,
            agilitate: 4,
            vitalitate: 2,
            noroc: 2,
            atac: 3,
            aparare: 2,
            carisma: 1,
            magie: 1,
        },
        0.5,
        &[],
        CutoutTemplate::Enemy,
        false,
        "Strigoiul s-a sculat din mormânt cu chef de harță.",
        STRIGOI_HUE,
    ),
    opponent(
        "Vârcolac",
        3,
        Attributes {
            putere: 4,
            agilitate: 4,
            vitalitate: 3,
            noroc: 2,
            atac: 5,
            aparare: 2,
            carisma: 1,
            magie: 1,
        },
        0.8,
        &[],
        CutoutTemplate::Enemy,
        false,
        "Vârcolacul a mirosit sânge proaspăt în arenă.",
        VARCOLAC_HUE,
    ),
    opponent(
        "Căpcăun",
        4,
        Attributes {
            putere: 7,
            agilitate: 2,
            vitalitate: 5,
            noroc: 2,
            atac: 4,
            aparare: 5,
            carisma: 1,
            magie: 1,
        },
        0.6,
        &[],
        CutoutTemplate::Enemy,
        false,
        "Căpcăunul n-a mai mâncat de aseară. Ghinion pentru cine-i iese în cale.",
        CAPCAUN_HUE,
    ),
    opponent(
        "Muma Pădurii",
        5,
        Attributes {
            putere: 4,
            agilitate: 3,
            vitalitate: 9,
            noroc: 6,
            atac: 3,
            aparare: 5,
            carisma: 3,
            magie: 4,
        },
        0.45,
        &[],
        CutoutTemplate::Boss,
        true,
        "Se întunecă senin: Muma Pădurii iese din desiș, iar codrul tace!",
        MUMA_PADURII_HUE,
    ),
    opponent(
        "Iele",
        6,
        Attributes {
            putere: 4,
            agilitate: 8,
            vitalitate: 4,
            noroc: 4,
            atac: 5,
            aparare: 4,
            carisma: 4,
            magie: 4,
        },
        0.55,
        &[],
        CutoutTemplate::Enemy,
        false,
        "Ielele dansează în arenă — cine le calcă hora, pățește.",
        IELE_HUE,
    ),
    opponent(
        "Solomonar",
        7,
        Attributes {
            putere: 5,
            agilitate: 4,
            vitalitate: 6,
            noroc: 6,
            atac: 5,
            aparare: 4,
            carisma: 4,
            magie: 8,
        },
        0.5,
        &[],
        CutoutTemplate::Human,
        false,
        "Solomonarul a coborât de pe nori, cu grindina în traistă.",
        SOLOMONAR_HUE,
    ),
    opponent(
        "Balaur cu trei capete",
        8,
        Attributes {
            putere: 9,
            agilitate: 4,
            vitalitate: 9,
            noroc: 4,
            atac: 8,
            aparare: 8,
            carisma: 2,
            magie: 3,
        },
        0.7,
        &[],
        CutoutTemplate::Boss,
        false,
        "Trei capete, un singur gând: balaurul vrea prânzul.",
        BALAUR_HUE,
    ),
    opponent(
        "Zmeu",
        9,
        Attributes {
            putere: 9,
            agilitate: 7,
            vitalitate: 8,
            noroc: 5,
            atac: 9,
            aparare: 8,
            carisma: 4,
            magie: 2,
        },
        0.85,
        &[ItemId::Palos],
        CutoutTemplate::Boss,
        false,
        "Zmeul a furat soarele și acum vrea și arena.",
        ZMEU_HUE,
    ),
    opponent(
        "Zmeul Zmeilor",
        10,
        Attributes {
            putere: 11,
            agilitate: 8,
            vitalitate: 11,
            noroc: 8,
            atac: 10,
            aparare: 9,
            carisma: 5,
            magie: 5,
        },
        0.75,
        &[ItemId::BuzduganCuTreiPeceti, ItemId::CamasaDeZale],
        CutoutTemplate::Boss,
        true,
        "Cutremur în arenă: Zmeul Zmeilor, spaima voinicilor, intră cu buzduganul în mână!",
        ZMEUL_ZMEILOR_HUE,
    ),
];

/// The attribute-point budget of an opponent: `7 + 5 * level` for regular
/// creatures, `7 + 6 * level` for bosses (#128 widened both, see the module
/// docs).
pub fn attribute_budget(level: u32, is_boss: bool) -> u32 {
    let per_level = if is_boss {
        BOSS_POINTS_PER_LEVEL
    } else {
        POINTS_PER_LEVEL
    };
    BUDGET_BASE + per_level * level
}

/// Total attribute points of a spread, over all eight kinds.
pub fn attribute_total(attrs: &Attributes) -> u32 {
    attrs.total()
}

/// Scales `base` for the given 1-based `lap`: the attribute total grows by
/// [`LAP_BONUS_PERCENT`] per completed lap, rounded half-up, and the growth
/// is distributed over the attributes by largest remainder (ties resolved in
/// [`AttributeKind::ALL`] order), so the scaled total always matches the
/// rounded target exactly. Lap 1 is the identity.
pub fn scaled_attributes(base: &Attributes, lap: u32) -> Attributes {
    // Float-free integer math: the multiplier is `numerator / 100`.
    let numerator = (100 + LAP_BONUS_PERCENT * lap.saturating_sub(1)) as u64;
    let scaled = AttributeKind::ALL.map(|kind| base.get(kind) as u64 * numerator);
    let target = (scaled.iter().sum::<u64>() + 50) / 100;
    let mut values = scaled.map(|value| value / 100);
    let mut extra = target - values.iter().sum::<u64>();
    let mut order = [0usize, 1, 2, 3, 4, 5, 6, 7];
    order.sort_by_key(|&i| std::cmp::Reverse(scaled[i] % 100));
    for &i in &order {
        if extra == 0 {
            break;
        }
        values[i] += 1;
        extra -= 1;
    }
    let mut result = Attributes::default();
    for (index, kind) in AttributeKind::ALL.into_iter().enumerate() {
        *result.get_mut(kind) = values[index] as u32;
    }
    result
}

/// Marker (plus intro line) for a boss fighter entity: the arena attaches it
/// so the label turns the boss color and the announcer opens with the boss's
/// own intro line.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boss {
    /// The announcer line this boss enters the arena with.
    pub intro_line: &'static str,
}

/// The run's position on the ladder: the 0-based index of the *next*
/// opponent, advanced on victory (see the progression flow) and reset with
/// the run. Indices past the ladder's end wrap into further laps with
/// scaled attributes.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LadderProgress(pub usize);

impl LadderProgress {
    /// The opponent of the current fight.
    pub fn opponent(&self) -> &'static Opponent {
        &LADDER[self.0 % LADDER.len()]
    }

    /// The 1-based lap of the current fight: 1 for the first pass over the
    /// ladder, 2 after beating opponent 10, and so on.
    pub fn lap(&self) -> u32 {
        (self.0 / LADDER.len()) as u32 + 1
    }

    /// The current opponent's attributes, scaled for the current lap.
    pub fn attributes(&self) -> Attributes {
        scaled_attributes(&self.opponent().attrs, self.lap())
    }

    /// The label text for the current opponent: the plain name on the first
    /// lap, "Nume (Turul N)" from the second lap on.
    pub fn display_name(&self) -> String {
        let opponent = self.opponent();
        match self.lap() {
            1 => opponent.name.to_string(),
            lap => format!("{} (Turul {lap})", opponent.name),
        }
    }

    /// Moves to the next opponent; called on victory.
    pub fn advance(&mut self) {
        self.0 += 1;
    }

    /// Whether the current fight is the last ladder entry on lap 1 — the
    /// lap-1 final boss whose defeat ends the run with the victory screen
    /// (#26). Later laps loop as usual.
    pub fn is_final_lap_one_fight(&self) -> bool {
        self.0 == LADDER.len() - 1
    }

    /// Generates the one representative human opponent migrated by #319.
    /// Every other ladder entry, including the Solomonar human template,
    /// returns `None` and stays on the existing authored-template path.
    pub fn seeded_opponent(
        &self,
        campaign_seed: CampaignSeed,
    ) -> Option<Result<SeededOpponent, GenerationError>> {
        self.0
            .is_multiple_of(LADDER.len())
            .then(|| hot_de_codru(campaign_seed))
    }
}

fn hot_de_codru(campaign_seed: CampaignSeed) -> Result<SeededOpponent, GenerationError> {
    let seed = derive_encounter_seed(campaign_seed.0, HOT_DE_CODRU_ENCOUNTER_ID);
    let profile = hot_de_codru_profile();
    let definition = generate_character(seed, &profile, bundled_human_catalog()?)?;

    Ok(SeededOpponent {
        encounter_id: HOT_DE_CODRU_ENCOUNTER_ID,
        seed,
        definition,
    })
}

fn bundled_human_catalog() -> Result<&'static CharacterCatalog, GenerationError> {
    static CATALOG: OnceLock<Result<CharacterCatalog, CatalogError>> = OnceLock::new();
    match CATALOG.get_or_init(|| {
        CharacterCatalog::from_json(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
    }) {
        Ok(catalog) => Ok(catalog),
        Err(error) => Err(error.clone().into()),
    }
}

fn hot_de_codru_profile() -> GenerationProfile {
    let appearance = PlayerAppearance::default();
    let legacy = CharacterDefinition::legacy_human(appearance);
    let hair_candidates = HairStyle::ALL
        .into_iter()
        .map(|hair| {
            let definition =
                CharacterDefinition::legacy_human(PlayerAppearance { hair, ..appearance });
            WeightedPart::new(definition.parts.hair, 1)
        })
        .collect();
    let mut slots = vec![
        locked_slot(BodyRegion::Body, legacy.parts.body),
        locked_slot(BodyRegion::Face, legacy.parts.face),
        GenerationSlot::new(BodyRegion::Hair, hair_candidates),
        locked_slot(BodyRegion::Torso, legacy.parts.torso),
        locked_slot(BodyRegion::Legs, legacy.parts.legs),
        locked_slot(BodyRegion::Feet, legacy.parts.feet),
    ];
    if let Some(waist) = legacy.parts.waist {
        slots.push(locked_slot(BodyRegion::Waist, waist));
    }

    GenerationProfile::new(
        SkeletonFamily::Human,
        CulturalProfile {
            tags: vec!["romanian".to_owned()],
        },
        appearance,
        slots,
    )
}

fn locked_slot(region: BodyRegion, id: PartId) -> GenerationSlot {
    GenerationSlot::new(region, vec![WeightedPart::new(id, 1)])
}

/// Stable FNV-1a derivation over the little-endian campaign seed followed by
/// the authored encounter ID. This intentionally avoids randomized standard
/// hashers so the same inputs remain portable across native and wasm builds.
fn derive_encounter_seed(campaign_seed: u64, encounter_id: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    campaign_seed
        .to_le_bytes()
        .into_iter()
        .chain(encounter_id.bytes())
        .fold(FNV_OFFSET, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
        })
}

/// Registers the ladder-position resource; the ladder itself is static data.
pub struct RosterPlugin;

impl Plugin for RosterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LadderProgress>()
            .init_resource::<CampaignSeed>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::Slot;

    #[test]
    fn the_ladder_names_the_ten_creatures_in_spec_order() {
        let names: Vec<&str> = LADDER.iter().map(|opponent| opponent.name).collect();
        assert_eq!(
            names,
            vec![
                "Hoț de codru",
                "Strigoi",
                "Vârcolac",
                "Căpcăun",
                "Muma Pădurii",
                "Iele",
                "Solomonar",
                "Balaur cu trei capete",
                "Zmeu",
                "Zmeul Zmeilor",
            ]
        );
    }

    #[test]
    fn levels_increase_strictly_from_one_to_ten() {
        for (index, opponent) in LADDER.iter().enumerate() {
            assert_eq!(
                opponent.level,
                index as u32 + 1,
                "{} sits at index {index}",
                opponent.name
            );
        }
    }

    #[test]
    fn exactly_the_fifth_and_tenth_opponents_are_bosses() {
        for (index, opponent) in LADDER.iter().enumerate() {
            assert_eq!(
                opponent.is_boss,
                index == 4 || index == 9,
                "{} (index {index})",
                opponent.name
            );
        }
    }

    #[test]
    fn every_spread_matches_its_budget() {
        for opponent in &LADDER {
            assert_eq!(
                attribute_total(&opponent.attrs),
                attribute_budget(opponent.level, opponent.is_boss),
                "{} (level {}, boss: {})",
                opponent.name,
                opponent.level,
                opponent.is_boss
            );
        }
    }

    #[test]
    fn the_budget_formulas_match_the_spec() {
        assert_eq!(attribute_budget(1, false), 12, "7 + 5 * 1");
        assert_eq!(attribute_budget(9, false), 52);
        assert_eq!(attribute_budget(5, true), 37, "7 + 6 * 5");
        assert_eq!(attribute_budget(10, true), 67);
    }

    /// [`BUDGET_BASE`] is defined as "every fighter's unallocated total";
    /// this pins it to the character model's own base values so the two
    /// can't drift apart silently.
    #[test]
    fn the_budget_base_matches_the_character_models_base_total() {
        assert_eq!(BUDGET_BASE, AttributeKind::base_total());
    }

    #[test]
    fn every_aggression_is_a_valid_profile_knob() {
        for opponent in &LADDER {
            assert!(
                (0.0..=1.0).contains(&opponent.aggression),
                "{} aggression {}",
                opponent.name,
                opponent.aggression
            );
        }
    }

    #[test]
    fn equipment_ids_resolve_in_the_catalog_without_slot_clashes() {
        for opponent in &LADDER {
            let mut slots: Vec<Slot> = Vec::new();
            for &id in opponent.equipment {
                let item = id.item();
                assert_eq!(item.id, id, "{}: {id:?} resolves", opponent.name);
                assert!(
                    !slots.contains(&item.slot),
                    "{}: two items in {:?}",
                    opponent.name,
                    item.slot
                );
                slots.push(item.slot);
            }
        }
    }

    #[test]
    fn the_zmeu_carries_the_palos_and_the_boss_zmeu_its_full_kit() {
        assert_eq!(LADDER[8].equipment, &[ItemId::Palos]);
        assert_eq!(
            LADDER[9].equipment,
            &[ItemId::BuzduganCuTreiPeceti, ItemId::CamasaDeZale]
        );
        for opponent in &LADDER[..8] {
            assert!(
                opponent.equipment.is_empty(),
                "{} fights bare",
                opponent.name
            );
        }
    }

    #[test]
    fn every_intro_line_is_plain_filled_text() {
        for opponent in &LADDER {
            assert!(!opponent.intro_line.is_empty(), "{}", opponent.name);
            assert!(
                !opponent.intro_line.contains('{'),
                "{} intro carries an unfilled placeholder",
                opponent.name
            );
        }
    }

    #[test]
    fn progress_starts_at_the_first_opponent_and_advances_in_order() {
        let mut progress = LadderProgress::default();
        assert_eq!(progress.opponent().name, "Hoț de codru");
        assert_eq!(progress.lap(), 1);
        for expected in &LADDER {
            assert_eq!(progress.opponent(), expected);
            progress.advance();
        }
        assert_eq!(progress, LadderProgress(10));
    }

    #[test]
    fn past_the_tenth_win_the_ladder_loops_into_the_next_lap() {
        let progress = LadderProgress(10);
        assert_eq!(progress.opponent().name, "Hoț de codru");
        assert_eq!(progress.lap(), 2);
        assert_eq!(LadderProgress(19).lap(), 2);
        assert_eq!(LadderProgress(20).lap(), 3);
        assert_eq!(LadderProgress(24).opponent().name, "Muma Pădurii");
    }

    #[test]
    fn only_the_tenth_fight_of_lap_one_is_the_final_lap_one_fight() {
        assert!(LadderProgress(9).is_final_lap_one_fight());
        assert!(!LadderProgress(0).is_final_lap_one_fight());
        assert!(!LadderProgress(8).is_final_lap_one_fight());
        assert!(
            !LadderProgress(19).is_final_lap_one_fight(),
            "the lap-2 Zmeul Zmeilor keeps the loop behavior"
        );
    }

    #[test]
    fn lap_labels_appear_from_the_second_lap_on() {
        assert_eq!(LadderProgress(0).display_name(), "Hoț de codru");
        assert_eq!(LadderProgress(9).display_name(), "Zmeul Zmeilor");
        assert_eq!(LadderProgress(10).display_name(), "Hoț de codru (Turul 2)");
        assert_eq!(LadderProgress(24).display_name(), "Muma Pădurii (Turul 3)");
    }

    #[test]
    fn the_first_lap_leaves_attributes_untouched() {
        for opponent in &LADDER {
            assert_eq!(scaled_attributes(&opponent.attrs, 1), opponent.attrs);
        }
    }

    #[test]
    fn every_second_lap_total_is_the_rounded_twenty_percent_bump() {
        for (lap, percent) in [(2, 120), (3, 140), (4, 160)] {
            for opponent in &LADDER {
                let base = attribute_total(&opponent.attrs);
                let scaled = attribute_total(&scaled_attributes(&opponent.attrs, lap));
                let expected = (base * percent + 50) / 100;
                assert_eq!(
                    scaled, expected,
                    "{} lap {lap}: total {base} must scale to {expected}",
                    opponent.name
                );
                assert!(
                    scaled > base,
                    "{} lap {lap} must be stronger",
                    opponent.name
                );
            }
        }
    }

    #[test]
    fn lap_scaling_distributes_by_largest_remainder() {
        // Hoț de codru 2/2/2/1/2/1/1/1 (total 12) on lap 2: target
        // round(14.4) = 14; every attribute floors (sum 12), and the two
        // extra points land on putere and agilitate (the first of the tied
        // largest remainders, in AttributeKind::ALL order).
        assert_eq!(
            scaled_attributes(&LADDER[0].attrs, 2),
            Attributes {
                putere: 3,
                agilitate: 3,
                vitalitate: 2,
                noroc: 1,
                atac: 2,
                aparare: 1,
                carisma: 1,
                magie: 1,
            }
        );
    }

    #[test]
    fn every_opponent_declares_exactly_one_accent_hue_with_no_adjacent_duplicates() {
        // #118: the roster has no visual identity without a per-opponent
        // accent hue. Every entry must carry one, and consecutive ladder
        // opponents (the ones a player sees back-to-back) must not share it.
        for opponent in &LADDER {
            assert_ne!(
                opponent.accent_hue,
                Color::WHITE,
                "{} must declare a real accent hue, not an untinted placeholder",
                opponent.name
            );
        }
        for pair in LADDER.windows(2) {
            assert_ne!(
                pair[0].accent_hue, pair[1].accent_hue,
                "{} and {} are adjacent on the ladder and must not share an accent hue",
                pair[0].name, pair[1].name
            );
        }
    }

    #[test]
    fn no_attribute_ever_shrinks_across_laps() {
        for lap in 1..=5 {
            for opponent in &LADDER {
                let scaled = scaled_attributes(&opponent.attrs, lap);
                for kind in AttributeKind::ALL {
                    assert!(
                        scaled.get(kind) >= opponent.attrs.get(kind),
                        "{} lap {lap}: {kind:?}",
                        opponent.name
                    );
                }
            }
        }
    }
}
