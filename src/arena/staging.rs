//! Persistent positional staging (combat redesign §2, see
//! `docs/combat-redesign-proposal.md`): the presentation-side single source
//! of truth for where the two fighters stand. The engine's shared
//! [`DuelDistance`] band stays untouched — this resource only translates
//! each band into a concrete fighter-center gap and keeps both world x
//! positions across the whole fight, so spacing is readable on the stage
//! itself instead of only in the log. Fighters never return to fixed
//! anchors: absolute positions are path-dependent (only the gap is
//! band-determined).

use bevy::prelude::*;

use crate::combat::{CombatSide, DuelDistance};

/// Fighter-center gap at [`DuelDistance::CLOSE`], in world units.
pub const CLOSE_GAP: f32 = 140.0;
/// Fighter-center gap at [`DuelDistance::NEAR`], in world units.
pub const NEAR_GAP: f32 = 250.0;
/// Fighter-center gap at [`DuelDistance::FAR`], in world units.
pub const FAR_GAP: f32 = 360.0;

/// Rightward bias of the initial pair placement: the fight opens centered
/// on this x rather than 0, keeping the left band of the stage clear for
/// the action palette (§3 of the redesign proposal).
pub const STAGE_BIAS: f32 = 40.0;

/// Left wall for fighter centers; the strip left of it is reserved for the
/// action palette (§3).
pub const STAGE_MIN_X: f32 = -150.0;
/// Right wall for fighter centers, mirroring [`STAGE_MIN_X`] inside the
/// 800-unit stage.
pub const STAGE_MAX_X: f32 = 330.0;

/// The concrete fighter-center gap for one engine distance band.
pub fn band_gap(distance: DuelDistance) -> f32 {
    if distance.band() == DuelDistance::CLOSE.band() {
        CLOSE_GAP
    } else if distance.band() == DuelDistance::NEAR.band() {
        NEAR_GAP
    } else {
        FAR_GAP
    }
}

/// Where the two fighters stand, as world x of each fighter's center. The
/// player is always left of the enemy; the sides never cross. Updated only
/// by [`ArenaStaging::apply_move`] (from `CombatEvent::Moved`), so the
/// positions are deterministic per event sequence — the frozen desktop
/// fight helper relies on that.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ArenaStaging {
    /// World x of the player fighter's center (the left fighter).
    pub player_x: f32,
    /// World x of the enemy fighter's center (the right fighter).
    pub enemy_x: f32,
    distance: DuelDistance,
}

impl Default for ArenaStaging {
    fn default() -> Self {
        Self::starting()
    }
}

impl ArenaStaging {
    /// The opening placement: both fighters centered on [`STAGE_BIAS`] at
    /// the engine's starting band.
    pub fn starting() -> Self {
        let distance = DuelDistance::starting();
        let gap = band_gap(distance);
        Self {
            player_x: STAGE_BIAS - gap / 2.0,
            enemy_x: STAGE_BIAS + gap / 2.0,
            distance,
        }
    }

    /// The staged x of `side`'s fighter center.
    pub fn x_of(&self, side: CombatSide) -> f32 {
        match side {
            CombatSide::Player => self.player_x,
            CombatSide::Enemy => self.enemy_x,
        }
    }

    /// The band the current gap realizes — the same value the engine's
    /// `CombatTurn::distance` holds after the last `Moved` event.
    pub fn distance(&self) -> DuelDistance {
        self.distance
    }

    /// Current fighter-center gap; always exactly [`band_gap`] of
    /// [`Self::distance`].
    pub fn gap(&self) -> f32 {
        self.enemy_x - self.player_x
    }

    /// The x centered between the two fighters — where the ground distance
    /// chip sits.
    pub fn midpoint_x(&self) -> f32 {
        (self.player_x + self.enemy_x) / 2.0
    }

    /// Applies one `CombatEvent::Moved { to, .. }`: only the actor moves,
    /// to exactly [`band_gap`]`(to)` from its (standing) opponent. If the
    /// actor's target would cross a stage wall, the residual shifts *both*
    /// fighters (pair slide) so the gap stays exact — spacing is truth,
    /// absolute position is composition.
    pub fn apply_move(&mut self, actor: CombatSide, to: DuelDistance) {
        let gap = band_gap(to);
        match actor {
            CombatSide::Player => self.player_x = self.enemy_x - gap,
            CombatSide::Enemy => self.enemy_x = self.player_x + gap,
        }
        let residual = if self.player_x < STAGE_MIN_X {
            STAGE_MIN_X - self.player_x
        } else if self.enemy_x > STAGE_MAX_X {
            STAGE_MAX_X - self.enemy_x
        } else {
            0.0
        };
        self.player_x += residual;
        self.enemy_x += residual;
        self.distance = to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_band_maps_to_its_documented_gap() {
        assert_eq!(band_gap(DuelDistance::CLOSE), 140.0);
        assert_eq!(band_gap(DuelDistance::NEAR), 250.0);
        assert_eq!(band_gap(DuelDistance::FAR), 360.0);
    }

    #[test]
    fn the_fight_opens_centered_on_the_stage_bias_at_the_starting_band() {
        let staging = ArenaStaging::starting();
        assert_eq!(staging.player_x, -30.0);
        assert_eq!(staging.enemy_x, 110.0);
        assert_eq!(staging.gap(), band_gap(DuelDistance::starting()));
        assert_eq!(staging.midpoint_x(), STAGE_BIAS);
        assert!(staging.player_x < staging.enemy_x, "player stays left");
    }

    #[test]
    fn only_the_actor_moves_and_the_gap_becomes_exactly_the_target_band() {
        let mut staging = ArenaStaging::starting();
        staging.apply_move(CombatSide::Player, DuelDistance::NEAR);
        assert_eq!(staging.enemy_x, 110.0, "the standing opponent never moves");
        assert_eq!(staging.player_x, 110.0 - NEAR_GAP);
        assert_eq!(staging.gap(), NEAR_GAP);
        assert_eq!(staging.distance(), DuelDistance::NEAR);

        let mut staging = ArenaStaging::starting();
        staging.apply_move(CombatSide::Enemy, DuelDistance::NEAR);
        assert_eq!(staging.player_x, -30.0, "the standing opponent never moves");
        assert_eq!(staging.enemy_x, -30.0 + NEAR_GAP);
        assert_eq!(staging.gap(), NEAR_GAP);
    }

    #[test]
    fn a_left_wall_hit_slides_the_pair_right_keeping_the_gap_exact() {
        let mut staging = ArenaStaging::starting();
        staging.apply_move(CombatSide::Player, DuelDistance::NEAR);
        // Raw target: 110 - 360 = -250, which crosses the -150 wall by 100;
        // the pair slides right together.
        staging.apply_move(CombatSide::Player, DuelDistance::FAR);
        assert_eq!(staging.player_x, STAGE_MIN_X);
        assert_eq!(staging.enemy_x, STAGE_MIN_X + FAR_GAP);
        assert_eq!(staging.gap(), FAR_GAP, "the gap is exact after the slide");
    }

    #[test]
    fn a_right_wall_hit_slides_the_pair_left_keeping_the_gap_exact() {
        // Walk the pair rightwards first: the enemy retreats, the player
        // chases back to close, leaving the pair at (80, 220).
        let mut staging = ArenaStaging::starting();
        staging.apply_move(CombatSide::Enemy, DuelDistance::NEAR);
        staging.apply_move(CombatSide::Player, DuelDistance::CLOSE);
        assert_eq!((staging.player_x, staging.enemy_x), (80.0, 220.0));
        // Raw target: 80 + 360 = 440, which crosses the +330 wall by 110;
        // the pair slides left together.
        staging.apply_move(CombatSide::Enemy, DuelDistance::FAR);
        assert_eq!(staging.enemy_x, STAGE_MAX_X);
        assert_eq!(staging.player_x, STAGE_MAX_X - FAR_GAP);
        assert_eq!(staging.gap(), FAR_GAP, "the gap is exact after the slide");
    }

    #[test]
    fn positions_are_path_dependent_only_the_gap_is_band_determined() {
        // Enemy retreats, player steps after: back at close, but the pair
        // stands somewhere else than at the fight's opening — stepping
        // forward after a step back must NOT restore the original absolute
        // positions unless the math happens to land there.
        let start = ArenaStaging::starting();
        let mut staging = start;
        staging.apply_move(CombatSide::Enemy, DuelDistance::NEAR);
        staging.apply_move(CombatSide::Player, DuelDistance::CLOSE);
        assert_eq!(staging.gap(), start.gap(), "the gap is band-determined");
        assert_ne!(
            (staging.player_x, staging.enemy_x),
            (start.player_x, start.enemy_x),
            "absolute positions drifted with the movement history"
        );
    }

    #[test]
    fn walls_are_never_crossed_across_arbitrary_movement_sequences() {
        let mut staging = ArenaStaging::starting();
        let moves = [
            (CombatSide::Player, DuelDistance::NEAR),
            (CombatSide::Player, DuelDistance::FAR),
            (CombatSide::Enemy, DuelDistance::NEAR),
            (CombatSide::Enemy, DuelDistance::FAR),
            (CombatSide::Player, DuelDistance::CLOSE),
            (CombatSide::Enemy, DuelDistance::FAR),
            (CombatSide::Enemy, DuelDistance::CLOSE),
        ];
        for (actor, to) in moves {
            staging.apply_move(actor, to);
            assert!(staging.player_x >= STAGE_MIN_X, "left wall holds");
            assert!(staging.enemy_x <= STAGE_MAX_X, "right wall holds");
            assert_eq!(staging.gap(), band_gap(to), "the gap is always exact");
            assert!(staging.player_x < staging.enemy_x, "sides never cross");
        }
    }
}
