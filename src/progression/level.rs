//! Pure leveling rules: the XP curve, level-up grants, the vitalitate pool
//! top-up, and the point-allocation draft for the level-up screen.
//!
//! No ECS systems here: like [`crate::creation::draft`], these are plain
//! value types (registered as resources by the plugin) so the invariants —
//! surplus XP carries over, multi-level-ups all grant points, allocation
//! never overspends or drops below the confirmed build — are unit-testable
//! without a `World`.

use bevy::prelude::*;

use crate::character::{AttributeKind, Attributes};

/// Unspent attribute points granted per level gained.
pub const POINTS_PER_LEVEL: u32 = 2;

/// XP required to go from `level` to `level + 1`:
/// `100 + 50 * (level - 1) * level / 2` — 100, 150, 250, 400, … a
/// cumulative-ish ramp.
pub fn xp_to_next(level: u32) -> u32 {
    100 + 50 * (level - 1) * level / 2
}

/// Tops up a pool after its maximum changed: the current value moves by
/// exactly the max-delta, so a level-up never heals more (or less) than the
/// growth itself. Returns the `(current, max)` pair after the change.
pub fn top_up_pool(current: i32, max: i32, new_max: i32) -> (i32, i32) {
    (current + (new_max - max), new_max)
}

/// The player's experience state: current level, XP progress towards the
/// next one, and attribute points not yet allocated. Lives alongside
/// [`crate::creation::PlayerCharacter`] and resets with the run.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    /// Current level, starting at 1.
    pub level: u32,
    /// XP gathered towards the next level; always below
    /// [`xp_to_next`]`(self.level)`.
    pub xp: u32,
    /// Attribute points granted by level-ups and not yet spent.
    pub unspent_points: u32,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            unspent_points: 0,
        }
    }
}

impl Level {
    /// Adds `amount` XP, applying every level-up it affords: surplus XP
    /// carries over and each level grants [`POINTS_PER_LEVEL`] unspent
    /// points. Returns how many levels were gained.
    pub fn gain_xp(&mut self, amount: u32) -> u32 {
        self.xp += amount;
        let mut gained = 0;
        while self.xp >= xp_to_next(self.level) {
            self.xp -= xp_to_next(self.level);
            self.level += 1;
            self.unspent_points += POINTS_PER_LEVEL;
            gained += 1;
        }
        gained
    }
}

/// The in-progress point allocation on the level-up panel. Fields are
/// private so every mutation goes through methods that uphold the
/// invariants: never spend more than the granted points, never drop an
/// attribute below the already-confirmed build.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct LevelUpDraft {
    base: Attributes,
    attributes: Attributes,
    points_remaining: u32,
}

impl LevelUpDraft {
    /// Starts an allocation of `points` on top of the confirmed `base`
    /// attributes.
    pub fn new(base: Attributes, points: u32) -> Self {
        Self {
            base,
            attributes: base,
            points_remaining: points,
        }
    }

    /// The attributes as allocated so far.
    pub fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// Current value of one attribute.
    pub fn get(&self, kind: AttributeKind) -> u32 {
        self.attributes.get(kind)
    }

    /// Points still available.
    pub fn points_remaining(&self) -> u32 {
        self.points_remaining
    }

    /// Whether any attribute can still be raised (points remain).
    pub fn can_increase(&self) -> bool {
        self.points_remaining > 0
    }

    /// Whether `kind` can be lowered (it is above the confirmed build).
    pub fn can_decrease(&self, kind: AttributeKind) -> bool {
        self.attributes.get(kind) > self.base.get(kind)
    }

    /// Spends one point on `kind`. Returns whether the point was spent.
    pub fn increase(&mut self, kind: AttributeKind) -> bool {
        if !self.can_increase() {
            return false;
        }
        *self.attributes.get_mut(kind) += 1;
        self.points_remaining -= 1;
        true
    }

    /// Refunds one point from `kind`. Returns whether a point was refunded.
    pub fn decrease(&mut self, kind: AttributeKind) -> bool {
        if !self.can_decrease(kind) {
            return false;
        }
        *self.attributes.get_mut(kind) -= 1;
        self.points_remaining += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::stats;

    #[test]
    fn xp_to_next_follows_the_curve_for_levels_one_to_ten() {
        let expected = [
            (1, 100),
            (2, 150),
            (3, 250),
            (4, 400),
            (5, 600),
            (6, 850),
            (7, 1150),
            (8, 1500),
            (9, 1900),
            (10, 2350),
        ];
        for (level, xp) in expected {
            assert_eq!(xp_to_next(level), xp, "level {level}");
        }
    }

    #[test]
    fn a_fresh_level_is_one_with_nothing_gathered() {
        assert_eq!(
            Level::default(),
            Level {
                level: 1,
                xp: 0,
                unspent_points: 0,
            }
        );
    }

    #[test]
    fn gaining_below_the_threshold_only_gathers_xp() {
        let mut level = Level::default();
        assert_eq!(level.gain_xp(60), 0);
        assert_eq!(
            level,
            Level {
                level: 1,
                xp: 60,
                unspent_points: 0,
            }
        );
    }

    #[test]
    fn a_single_level_up_grants_two_points_and_carries_the_surplus() {
        let mut level = Level::default();
        assert_eq!(level.gain_xp(120), 1);
        assert_eq!(
            level,
            Level {
                level: 2,
                xp: 20,
                unspent_points: POINTS_PER_LEVEL,
            }
        );
    }

    #[test]
    fn hitting_the_threshold_exactly_levels_up_with_zero_surplus() {
        let mut level = Level::default();
        assert_eq!(level.gain_xp(100), 1);
        assert_eq!(
            level,
            Level {
                level: 2,
                xp: 0,
                unspent_points: POINTS_PER_LEVEL,
            }
        );
    }

    #[test]
    fn one_award_can_grant_multiple_level_ups() {
        // 100 (level 1→2) + 150 (level 2→3) + 10 surplus.
        let mut level = Level::default();
        assert_eq!(level.gain_xp(260), 2);
        assert_eq!(
            level,
            Level {
                level: 3,
                xp: 10,
                unspent_points: 2 * POINTS_PER_LEVEL,
            }
        );
    }

    #[test]
    fn points_accumulate_across_awards_when_left_unspent() {
        let mut level = Level::default();
        level.gain_xp(100);
        level.gain_xp(150);
        assert_eq!(level.level, 3);
        assert_eq!(level.unspent_points, 2 * POINTS_PER_LEVEL);
    }

    #[test]
    fn top_up_moves_current_by_exactly_the_max_delta() {
        // A wounded pool grows by the delta, not to full.
        assert_eq!(top_up_pool(50, 90, 110), (70, 110));
        // A full pool stays full.
        assert_eq!(top_up_pool(90, 90, 110), (110, 110));
        // No growth, no change.
        assert_eq!(top_up_pool(35, 90, 90), (35, 90));
    }

    #[test]
    fn top_up_matches_the_vitalitate_formulas() {
        // +2 vitalitate on the default build: max HP 60 → 80, stamina 35 → 45.
        let before = Attributes::default();
        let after = Attributes {
            vitalitate: before.vitalitate + 2,
            ..before
        };
        assert_eq!(
            top_up_pool(41, stats::max_hp(&before), stats::max_hp(&after)),
            (61, 80)
        );
        assert_eq!(
            top_up_pool(10, stats::max_stamina(&before), stats::max_stamina(&after)),
            (20, 45)
        );
    }

    #[test]
    fn a_fresh_draft_mirrors_the_confirmed_build() {
        let base = Attributes {
            putere: 5,
            agilitate: 1,
            vitalitate: 7,
            noroc: 1,
        };
        let draft = LevelUpDraft::new(base, 2);
        assert_eq!(draft.attributes(), base);
        assert_eq!(draft.points_remaining(), 2);
        assert!(draft.can_increase());
        for kind in AttributeKind::ALL {
            assert!(!draft.can_decrease(kind), "{kind:?} sits at the base");
        }
    }

    #[test]
    fn increase_spends_a_point() {
        let mut draft = LevelUpDraft::new(Attributes::default(), 2);
        assert!(draft.increase(AttributeKind::Vitalitate));
        assert_eq!(draft.get(AttributeKind::Vitalitate), 2);
        assert_eq!(draft.points_remaining(), 1);
    }

    #[test]
    fn cannot_overspend_past_the_granted_points() {
        let mut draft = LevelUpDraft::new(Attributes::default(), 2);
        assert!(draft.increase(AttributeKind::Putere));
        assert!(draft.increase(AttributeKind::Putere));
        assert!(!draft.can_increase());
        assert!(!draft.increase(AttributeKind::Putere), "no points left");
        assert_eq!(draft.get(AttributeKind::Putere), 3);
        assert_eq!(draft.points_remaining(), 0);
    }

    #[test]
    fn cannot_drop_below_the_confirmed_build() {
        let base = Attributes {
            putere: 5,
            ..Attributes::default()
        };
        let mut draft = LevelUpDraft::new(base, 2);
        assert!(!draft.decrease(AttributeKind::Putere), "5 is the floor");
        assert_eq!(draft.get(AttributeKind::Putere), 5);
        assert_eq!(draft.points_remaining(), 2, "nothing refunded");
    }

    #[test]
    fn decrease_refunds_a_point_spent_this_session() {
        let mut draft = LevelUpDraft::new(Attributes::default(), 2);
        draft.increase(AttributeKind::Noroc);
        assert!(draft.can_decrease(AttributeKind::Noroc));
        assert!(draft.decrease(AttributeKind::Noroc));
        assert_eq!(draft.get(AttributeKind::Noroc), 1);
        assert_eq!(draft.points_remaining(), 2);
    }

    #[test]
    fn a_zero_point_draft_allows_nothing() {
        let mut draft = LevelUpDraft::new(Attributes::default(), 0);
        assert!(!draft.can_increase());
        assert!(!draft.increase(AttributeKind::Agilitate));
        assert_eq!(draft.attributes(), Attributes::default());
    }
}
