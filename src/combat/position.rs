//! Continuous 1D duel geometry (#159): the *sole* positional model of a
//! fight, from combat rules through rendered fighter transforms. Each
//! fighter has one world-unit `x`; separation, melee reach, movement
//! clamping, and the arena walls are all derived from those two numbers.
//! There is no stored distance band anywhere in runtime combat — the HUD's
//! coarse Romanian distance label is a projection of the continuous
//! separation (see `super::hud::distance_label`).
//!
//! ## World units and the old-band equivalence
//!
//! The constants below preserve the pre-#159 band behavior exactly (the
//! former `DuelDistance` bands and the arena's former presentation-side
//! staging, which already realized them as world-unit gaps):
//!
//! | old band concept            | world-unit equivalent                    |
//! |-----------------------------|------------------------------------------|
//! | `CLOSE` gap                 | separation [`MIN_SEPARATION`] (140.0)    |
//! | `NEAR` gap                  | separation 250.0                         |
//! | `FAR` gap                   | separation [`MAX_SEPARATION`] (360.0)    |
//! | one band of movement        | [`STEP_DISTANCE`] (110.0)                |
//! | `LeapForward` (two bands)   | [`LEAP_DISTANCE`] (220.0)                |
//! | `in_melee_reach` (`CLOSE`)  | separation <= [`MELEE_REACH`] (140.0)    |
//! | band saturation at `FAR`    | separation clamped at [`MAX_SEPARATION`] |
//! | stage walls                 | [`ARENA_BOUNDS`] (-150.0 ..= 330.0)      |
//!
//! Because the old bands were evenly spaced in world units (110.0 apart),
//! delta-based continuous movement reproduces every position the band
//! model could reach, bit for bit.
//!
//! ## Invariants
//!
//! [`DuelPositions`] maintains, after every operation:
//! - **Ordering / no pass-through**: `player_x < enemy_x`, always separated
//!   by at least [`MIN_SEPARATION`].
//! - **Bounds**: both fighters inside [`ARENA_BOUNDS`]. A retreat whose
//!   target would cross a wall slides *both* fighters by the residual (the
//!   pair slide), keeping the separation exact — spacing is rules truth,
//!   absolute position is composition. (#160 replaces this with true wall
//!   pinning when retreat becomes tactical.)
//! - **Path dependence**: only separation is movement-determined; absolute
//!   positions carry the whole movement history.
//!
//! ## Handoff (for the #160 tactical-positioning child)
//!
//! The exported surface is: [`ArenaBounds`]/[`ARENA_BOUNDS`],
//! [`DuelPositions`] (`starting`, `x_of`, `separation`, `midpoint_x`,
//! `in_melee_reach`, `advance`, `retreat`), the movement/reach constants
//! above, and [`CombatSide`]. Transform synchronization: fighters spawn at
//! `arena::staged_fighter_transform(positions.x_of(side))` and
//! `arena::animation` tweens `Transform.translation.x` to exactly the
//! authoritative x carried by `CombatEvent::Moved { to, .. }`.

use bevy::prelude::Resource;

/// The two sides of a duel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatSide {
    Player,
    Enemy,
}

impl CombatSide {
    /// The other side of the duel.
    pub fn opponent(self) -> Self {
        match self {
            Self::Player => Self::Enemy,
            Self::Enemy => Self::Player,
        }
    }
}

/// The inclusive world-unit interval fighter centers may occupy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArenaBounds {
    /// Left wall for fighter centers; the strip left of it is reserved for
    /// the action palette (combat redesign §3).
    pub min_x: f32,
    /// Right wall for fighter centers, mirroring [`Self::min_x`] inside the
    /// 800-unit stage.
    pub max_x: f32,
}

impl ArenaBounds {
    /// Whether `x` lies inside the bounds.
    pub fn contains(&self, x: f32) -> bool {
        (self.min_x..=self.max_x).contains(&x)
    }
}

/// The duel's arena walls, in the same world units the arena stage is drawn
/// in.
pub const ARENA_BOUNDS: ArenaBounds = ArenaBounds {
    min_x: -150.0,
    max_x: 330.0,
};

/// Rightward bias of the opening placement: the fight opens centered on
/// this x rather than 0, keeping the left band of the stage clear for the
/// action palette (combat redesign §3).
pub const STAGE_BIAS: f32 = 40.0;

/// Closest the two fighter centers can stand — the no-pass-through buffer.
/// Equals the old `CLOSE` band's gap.
pub const MIN_SEPARATION: f32 = 140.0;

/// Farthest apart movement can place the fighters. Equals the old `FAR`
/// band's gap (band saturation); #160 owns replacing this cap with true
/// wall pinning.
pub const MAX_SEPARATION: f32 = 360.0;

/// Separation at or inside which melee strikes connect. Equals the old
/// "in reach only at `CLOSE`" rule.
pub const MELEE_REACH: f32 = MIN_SEPARATION;

/// Ground covered by `StepForward`/`StepBack` — the old bands' even
/// world-unit spacing.
pub const STEP_DISTANCE: f32 = 110.0;

/// Ground covered by `LeapForward` — exactly two steps, as the old
/// two-band leap was.
pub const LEAP_DISTANCE: f32 = 2.0 * STEP_DISTANCE;

/// Where the two fighters stand, as world x of each fighter's center — the
/// authoritative duel geometry. The player is always left of the enemy and
/// the sides never cross. Mutated only by the combat resolver (via
/// [`Self::advance`]/[`Self::retreat`]), so positions are deterministic per
/// action sequence; the arena reads it for spawning, lunges, and the ground
/// distance chip.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct DuelPositions {
    /// World x of the player fighter's center (the left fighter).
    pub player_x: f32,
    /// World x of the enemy fighter's center (the right fighter).
    pub enemy_x: f32,
}

impl Default for DuelPositions {
    fn default() -> Self {
        Self::starting()
    }
}

impl DuelPositions {
    /// The opening placement: both fighters centered on [`STAGE_BIAS`], one
    /// [`MIN_SEPARATION`] apart (toe-to-toe, in melee reach) — the same
    /// opening the band model realized.
    pub fn starting() -> Self {
        Self {
            player_x: STAGE_BIAS - MIN_SEPARATION / 2.0,
            enemy_x: STAGE_BIAS + MIN_SEPARATION / 2.0,
        }
    }

    /// The authoritative x of `side`'s fighter center.
    pub fn x_of(&self, side: CombatSide) -> f32 {
        match side {
            CombatSide::Player => self.player_x,
            CombatSide::Enemy => self.enemy_x,
        }
    }

    /// Derived distance between the two fighter centers. Never stored:
    /// always recomputed from the positions.
    pub fn separation(&self) -> f32 {
        self.enemy_x - self.player_x
    }

    /// Whether melee strikes can connect at the current separation.
    pub fn in_melee_reach(&self) -> bool {
        in_melee_reach(self.separation())
    }

    /// The x centered between the two fighters — where the ground distance
    /// chip sits.
    pub fn midpoint_x(&self) -> f32 {
        (self.player_x + self.enemy_x) / 2.0
    }

    /// Moves `side` `distance` world units toward its opponent, stopping at
    /// [`MIN_SEPARATION`] (no pass-through). The opponent never moves; a
    /// fighter advancing toward an in-bounds opponent can never leave the
    /// arena itself.
    pub fn advance(&mut self, side: CombatSide, distance: f32) {
        let separation = (self.separation() - distance).max(MIN_SEPARATION);
        self.place_at_separation(side, separation);
    }

    /// Moves `side` `distance` world units away from its opponent, clamped
    /// at [`MAX_SEPARATION`]. If the mover's target would cross an arena
    /// wall, the residual slides *both* fighters (pair slide) so the
    /// separation stays exact — see the module docs.
    pub fn retreat(&mut self, side: CombatSide, distance: f32) {
        let separation = (self.separation() + distance).min(MAX_SEPARATION);
        self.place_at_separation(side, separation);
        let residual = if self.player_x < ARENA_BOUNDS.min_x {
            ARENA_BOUNDS.min_x - self.player_x
        } else if self.enemy_x > ARENA_BOUNDS.max_x {
            ARENA_BOUNDS.max_x - self.enemy_x
        } else {
            0.0
        };
        self.player_x += residual;
        self.enemy_x += residual;
    }

    /// Re-places `side` at exactly `separation` from its standing opponent,
    /// preserving the player-left ordering.
    fn place_at_separation(&mut self, side: CombatSide, separation: f32) {
        match side {
            CombatSide::Player => self.player_x = self.enemy_x - separation,
            CombatSide::Enemy => self.enemy_x = self.player_x + separation,
        }
    }
}

/// Whether melee strikes connect across `separation` world units — the one
/// reach rule the resolver, the AI, and descriptor legality all share.
pub fn in_melee_reach(separation: f32) -> bool {
    separation <= MELEE_REACH
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Separation of the old `NEAR` band, used by the equivalence tests.
    const NEAR_EQUIVALENT: f32 = MIN_SEPARATION + STEP_DISTANCE;

    #[test]
    fn the_fight_opens_centered_on_the_stage_bias_in_melee_reach() {
        let positions = DuelPositions::starting();
        assert_eq!(positions.player_x, -30.0);
        assert_eq!(positions.enemy_x, 110.0);
        assert_eq!(positions.separation(), MIN_SEPARATION);
        assert_eq!(positions.midpoint_x(), STAGE_BIAS);
        assert!(positions.in_melee_reach());
        assert!(positions.player_x < positions.enemy_x, "player stays left");
        assert_eq!(DuelPositions::default(), DuelPositions::starting());
    }

    #[test]
    fn x_of_names_each_side_and_opponent_flips_it() {
        let positions = DuelPositions::starting();
        assert_eq!(positions.x_of(CombatSide::Player), positions.player_x);
        assert_eq!(positions.x_of(CombatSide::Enemy), positions.enemy_x);
        assert_eq!(CombatSide::Player.opponent(), CombatSide::Enemy);
        assert_eq!(CombatSide::Enemy.opponent(), CombatSide::Player);
    }

    #[test]
    fn melee_reach_is_a_pure_projection_of_separation() {
        assert!(in_melee_reach(MELEE_REACH));
        assert!(in_melee_reach(0.0));
        assert!(!in_melee_reach(MELEE_REACH + 0.5));
        assert!(!in_melee_reach(NEAR_EQUIVALENT));
        assert!(!in_melee_reach(MAX_SEPARATION));
    }

    #[test]
    fn a_retreat_covers_one_step_and_only_moves_the_actor() {
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.enemy_x, 110.0, "the standing opponent stays");
        assert_eq!(positions.player_x, -140.0);
        assert_eq!(positions.separation(), NEAR_EQUIVALENT);
        assert!(!positions.in_melee_reach());

        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        assert_eq!(positions.player_x, -30.0, "the standing opponent stays");
        assert_eq!(positions.enemy_x, -30.0 + NEAR_EQUIVALENT);
    }

    #[test]
    fn an_advance_covers_one_step_and_stops_at_the_minimum_separation() {
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        positions.advance(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.separation(), MIN_SEPARATION);

        // Advancing while already toe-to-toe is a no-op.
        let before = positions;
        positions.advance(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions, before, "no pass-through past MIN_SEPARATION");
    }

    #[test]
    fn a_leap_covers_two_steps_and_clamps_at_the_minimum_separation() {
        // From the old FAR equivalent, a leap lands exactly toe-to-toe.
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.separation(), MAX_SEPARATION);
        positions.advance(CombatSide::Player, LEAP_DISTANCE);
        assert_eq!(positions.separation(), MIN_SEPARATION);
        assert!(positions.in_melee_reach());

        // From the old NEAR equivalent, the same leap clamps to the same
        // toe-to-toe stop a single step reaches — the old band saturation.
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        positions.advance(CombatSide::Player, LEAP_DISTANCE);
        assert_eq!(positions.separation(), MIN_SEPARATION);
    }

    #[test]
    fn retreat_saturates_at_the_maximum_separation() {
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.separation(), MAX_SEPARATION);
        let before = positions;
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(
            positions, before,
            "retreating at max separation holds in place"
        );
    }

    #[test]
    fn a_left_wall_hit_slides_the_pair_right_keeping_the_separation_exact() {
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        // Raw target: -140 - 110 = -250, crossing the -150 wall by 100; the
        // pair slides right together.
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.player_x, ARENA_BOUNDS.min_x);
        assert_eq!(positions.enemy_x, ARENA_BOUNDS.min_x + MAX_SEPARATION);
        assert_eq!(positions.separation(), MAX_SEPARATION);
    }

    #[test]
    fn a_right_wall_hit_slides_the_pair_left_keeping_the_separation_exact() {
        // Walk the pair rightwards first: the enemy retreats, the player
        // chases back into reach, leaving the pair at (80, 220).
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.advance(CombatSide::Player, STEP_DISTANCE);
        assert_eq!((positions.player_x, positions.enemy_x), (80.0, 220.0));
        // Raw target: 80 + 360 = 440, crossing the +330 wall by 110; the
        // pair slides left together.
        positions.retreat(CombatSide::Enemy, LEAP_DISTANCE);
        assert_eq!(positions.enemy_x, ARENA_BOUNDS.max_x);
        assert_eq!(positions.player_x, ARENA_BOUNDS.max_x - MAX_SEPARATION);
        assert_eq!(positions.separation(), MAX_SEPARATION);
    }

    #[test]
    fn positions_are_path_dependent_only_separation_is_movement_determined() {
        // Enemy retreats, player steps after: back in reach, but the pair
        // stands somewhere else than at the fight's opening — advancing
        // after a retreat must NOT restore the original absolute positions.
        let start = DuelPositions::starting();
        let mut positions = start;
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.advance(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.separation(), start.separation());
        assert_ne!(
            (positions.player_x, positions.enemy_x),
            (start.player_x, start.enemy_x),
            "absolute positions drifted with the movement history"
        );
    }

    /// One scripted movement applied to the pair, for the invariant sweep.
    type Movement = fn(&mut DuelPositions, CombatSide);

    #[test]
    fn ordering_and_bounds_hold_across_arbitrary_movement_sequences() {
        let mut positions = DuelPositions::starting();
        let moves: [(CombatSide, Movement); 10] = [
            (CombatSide::Player, |p, s| p.retreat(s, STEP_DISTANCE)),
            (CombatSide::Player, |p, s| p.retreat(s, STEP_DISTANCE)),
            (CombatSide::Enemy, |p, s| p.retreat(s, STEP_DISTANCE)),
            (CombatSide::Player, |p, s| p.advance(s, LEAP_DISTANCE)),
            (CombatSide::Enemy, |p, s| p.retreat(s, LEAP_DISTANCE)),
            (CombatSide::Enemy, |p, s| p.advance(s, STEP_DISTANCE)),
            (CombatSide::Player, |p, s| p.advance(s, STEP_DISTANCE)),
            (CombatSide::Enemy, |p, s| p.retreat(s, STEP_DISTANCE)),
            (CombatSide::Player, |p, s| p.retreat(s, LEAP_DISTANCE)),
            (CombatSide::Enemy, |p, s| p.advance(s, LEAP_DISTANCE)),
        ];
        for (side, movement) in moves {
            movement(&mut positions, side);
            assert!(
                positions.player_x < positions.enemy_x,
                "ordering holds: {positions:?}"
            );
            assert!(
                positions.separation() >= MIN_SEPARATION,
                "no pass-through: {positions:?}"
            );
            assert!(
                positions.separation() <= MAX_SEPARATION,
                "separation cap holds: {positions:?}"
            );
            assert!(
                ARENA_BOUNDS.contains(positions.player_x)
                    && ARENA_BOUNDS.contains(positions.enemy_x),
                "bounds hold: {positions:?}"
            );
        }
    }

    #[test]
    fn arena_bounds_contains_is_inclusive() {
        assert!(ARENA_BOUNDS.contains(ARENA_BOUNDS.min_x));
        assert!(ARENA_BOUNDS.contains(ARENA_BOUNDS.max_x));
        assert!(ARENA_BOUNDS.contains(0.0));
        assert!(!ARENA_BOUNDS.contains(ARENA_BOUNDS.min_x - 1.0));
        assert!(!ARENA_BOUNDS.contains(ARENA_BOUNDS.max_x + 1.0));
    }
}
