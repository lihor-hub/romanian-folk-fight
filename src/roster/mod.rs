//! The folklore opponent ladder (#20): ten Romanian folklore creatures of
//! escalating difficulty, with a boss every five fights, as plain static
//! data. The arena spawns the current [`LadderProgress`] opponent, the
//! progression flow advances the ladder on victory, and after opponent 10
//! the ladder loops with every attribute total raised by 20% per lap
//! ("Turul 2", "Turul 3", ...).
//!
//! Attribute budgets are enforced by tests: a non-boss opponent of level `L`
//! carries `4 + 3 * L` total attribute points, a boss `4 + 4 * L`. The
//! spreads themselves are data, tuned per creature flavor.

use bevy::prelude::*;

use crate::character::Attributes;
use crate::items::ItemId;

/// Base attribute points every fighter starts from (1 per attribute).
const BUDGET_BASE: u32 = 4;
/// Attribute points per level for a non-boss opponent.
const POINTS_PER_LEVEL: u32 = 3;
/// Attribute points per level for a boss.
const BOSS_POINTS_PER_LEVEL: u32 = 4;
/// Extra attribute-total percent per completed lap of the ladder.
const LAP_BONUS_PERCENT: u32 = 20;

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
    /// Bosses get a distinct label color, an announcer intro, and double XP.
    pub is_boss: bool,
    /// The announcer's intro line; only shown for bosses.
    pub intro_line: &'static str,
}

/// Shorthand for a ladder entry; keeps the [`LADDER`] table readable.
#[allow(clippy::too_many_arguments)]
const fn opponent(
    name: &'static str,
    level: u32,
    putere: u32,
    agilitate: u32,
    vitalitate: u32,
    noroc: u32,
    aggression: f32,
    equipment: &'static [ItemId],
    is_boss: bool,
    intro_line: &'static str,
) -> Opponent {
    Opponent {
        name,
        level,
        attrs: Attributes {
            putere,
            agilitate,
            vitalitate,
            noroc,
        },
        aggression,
        equipment,
        is_boss,
        intro_line,
    }
}

/// The full ladder, in fight order. Budgets, monotonic levels, and the boss
/// positions are pinned by the integrity tests below.
pub static LADDER: [Opponent; 10] = [
    opponent(
        "Hoț de codru",
        1,
        2,
        2,
        2,
        1,
        0.25,
        &[],
        false,
        "Un hoț de codru pândește punga voinicului. Păzea!",
    ),
    opponent(
        "Strigoi",
        2,
        2,
        4,
        2,
        2,
        0.5,
        &[],
        false,
        "Strigoiul s-a sculat din mormânt cu chef de harță.",
    ),
    opponent(
        "Vârcolac",
        3,
        4,
        4,
        3,
        2,
        0.8,
        &[],
        false,
        "Vârcolacul a mirosit sânge proaspăt în arenă.",
    ),
    opponent(
        "Căpcăun",
        4,
        7,
        2,
        5,
        2,
        0.6,
        &[],
        false,
        "Căpcăunul n-a mai mâncat de aseară. Ghinion pentru cine-i iese în cale.",
    ),
    opponent(
        "Muma Pădurii",
        5,
        4,
        3,
        9,
        8,
        0.45,
        &[],
        true,
        "Se întunecă senin: Muma Pădurii iese din desiș, iar codrul tace!",
    ),
    opponent(
        "Iele",
        6,
        4,
        10,
        4,
        4,
        0.55,
        &[],
        false,
        "Ielele dansează în arenă — cine le calcă hora, pățește.",
    ),
    opponent(
        "Solomonar",
        7,
        6,
        5,
        6,
        8,
        0.5,
        &[],
        false,
        "Solomonarul a coborât de pe nori, cu grindina în traistă.",
    ),
    opponent(
        "Balaur cu trei capete",
        8,
        10,
        4,
        10,
        4,
        0.7,
        &[],
        false,
        "Trei capete, un singur gând: balaurul vrea prânzul.",
    ),
    opponent(
        "Zmeu",
        9,
        9,
        8,
        8,
        6,
        0.85,
        &[ItemId::Palos],
        false,
        "Zmeul a furat soarele și acum vrea și arena.",
    ),
    opponent(
        "Zmeul Zmeilor",
        10,
        12,
        10,
        12,
        10,
        0.75,
        &[ItemId::BuzduganCuTreiPeceti, ItemId::CamasaDeZale],
        true,
        "Cutremur în arenă: Zmeul Zmeilor, spaima voinicilor, intră cu buzduganul în mână!",
    ),
];

/// The attribute-point budget of an opponent: `4 + 3 * level` for regular
/// creatures, `4 + 4 * level` for bosses.
pub fn attribute_budget(level: u32, is_boss: bool) -> u32 {
    let per_level = if is_boss {
        BOSS_POINTS_PER_LEVEL
    } else {
        POINTS_PER_LEVEL
    };
    BUDGET_BASE + per_level * level
}

/// Total attribute points of a spread.
pub fn attribute_total(attrs: &Attributes) -> u32 {
    attrs.putere + attrs.agilitate + attrs.vitalitate + attrs.noroc
}

/// Scales `base` for the given 1-based `lap`: the attribute total grows by
/// [`LAP_BONUS_PERCENT`] per completed lap, rounded half-up, and the growth
/// is distributed over the attributes by largest remainder (ties resolved in
/// putere, agilitate, vitalitate, noroc order), so the scaled total always
/// matches the rounded target exactly. Lap 1 is the identity.
pub fn scaled_attributes(base: &Attributes, lap: u32) -> Attributes {
    // Float-free integer math: the multiplier is `numerator / 100`.
    let numerator = (100 + LAP_BONUS_PERCENT * lap.saturating_sub(1)) as u64;
    let scaled = [base.putere, base.agilitate, base.vitalitate, base.noroc]
        .map(|value| value as u64 * numerator);
    let target = (scaled.iter().sum::<u64>() + 50) / 100;
    let mut values = scaled.map(|value| value / 100);
    let mut extra = target - values.iter().sum::<u64>();
    let mut order = [0usize, 1, 2, 3];
    order.sort_by_key(|&i| std::cmp::Reverse(scaled[i] % 100));
    for &i in &order {
        if extra == 0 {
            break;
        }
        values[i] += 1;
        extra -= 1;
    }
    Attributes {
        putere: values[0] as u32,
        agilitate: values[1] as u32,
        vitalitate: values[2] as u32,
        noroc: values[3] as u32,
    }
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
}

/// Registers the ladder-position resource; the ladder itself is static data.
pub struct RosterPlugin;

impl Plugin for RosterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LadderProgress>();
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
        assert_eq!(attribute_budget(1, false), 7, "4 + 3 * 1");
        assert_eq!(attribute_budget(9, false), 31);
        assert_eq!(attribute_budget(5, true), 24, "4 + 4 * 5");
        assert_eq!(attribute_budget(10, true), 44);
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
        // Hoț de codru 2/2/2/1 (total 7) on lap 2: target round(8.4) = 8;
        // every attribute floors, the single extra point lands on putere
        // (first of the tied largest remainders).
        assert_eq!(
            scaled_attributes(&LADDER[0].attrs, 2),
            Attributes {
                putere: 3,
                agilitate: 2,
                vitalitate: 2,
                noroc: 1,
            }
        );
    }

    #[test]
    fn no_attribute_ever_shrinks_across_laps() {
        for lap in 1..=5 {
            for opponent in &LADDER {
                let scaled = scaled_attributes(&opponent.attrs, lap);
                assert!(scaled.putere >= opponent.attrs.putere);
                assert!(scaled.agilitate >= opponent.attrs.agilitate);
                assert!(scaled.vitalitate >= opponent.attrs.vitalitate);
                assert!(scaled.noroc >= opponent.attrs.noroc);
            }
        }
    }
}
