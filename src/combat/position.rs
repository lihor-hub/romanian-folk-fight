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
//!   target would cross the *mover's own* wall clamps that mover there —
//!   true wall pinning (#160): the standing opponent never moves as a side
//!   effect, so a fighter can walk itself into a corner with no space left
//!   to retreat further. [`DuelPositions::can_retreat`] is the legality
//!   check for that state; [`super::actions::action_disabled_reason`] reads
//!   it to disable the command and explain why.
//! - **Path dependence**: only separation is movement-determined; absolute
//!   positions carry the whole movement history.
//!
//! ## Tactical movement (#160)
//!
//! Ground covered by a step or leap scales with the *mover's own*
//! `agilitate`: [`step_distance`] and [`leap_distance`] replace the fixed
//! pre-#160 constants (kept as [`STEP_DISTANCE`]/[`LEAP_DISTANCE`], the
//! `agilitate == 1` baseline every position/arena/render fixture that
//! doesn't model a specific fighter still uses directly). A leap is always
//! [`LEAP_STEP_MULTIPLIER`] times that same fighter's own step, so it is
//! strictly larger than a step for every fighter at every legal agility.
//!
//! [`DuelPositions::displace_target`] is the one clamped primitive (arena
//! bounds + no-pass-through, no [`MAX_SEPARATION`] cap) later shove/recoil
//! integrations share, so neither reimplements this module's boundary
//! logic.
//!
//! ## Handoff (for the #130/#131/#135 children building on #160)
//!
//! The exported surface is: [`ArenaBounds`]/[`ARENA_BOUNDS`],
//! [`DuelPositions`] (`starting`, `x_of`, `separation`, `midpoint_x`,
//! `in_melee_reach`, `advance`, `retreat`, `can_retreat`, `is_wall_pinned`,
//! `retreat_space`, `displace_target`), the movement/reach constants above
//! plus [`step_distance`]/[`leap_distance`], and [`CombatSide`]. Transform
//! synchronization: fighters spawn at
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

/// Farthest apart movement can deliberately place the fighters. Equals the
/// old `FAR` band's gap (band saturation). A fighter can still end up
/// *closer* than this if its own arena wall binds first (#160's true wall
/// pinning) — this cap and the wall are independent constraints, and
/// [`DuelPositions::can_retreat`] checks both.
pub const MAX_SEPARATION: f32 = 360.0;

/// Separation at or inside which melee strikes connect. Equals the old
/// "in reach only at `CLOSE`" rule.
pub const MELEE_REACH: f32 = MIN_SEPARATION;

/// Baseline ground a step covers before `agilitate` scaling (#160): the
/// pre-#160 fixed distance, i.e. [`step_distance`]`(1)`.
pub const STEP_DISTANCE_BASE: f32 = 100.0;

/// Extra ground a step covers per point of `agilitate` above zero (#160).
pub const STEP_DISTANCE_PER_AGILITATE: f32 = 10.0;

/// How many steps a leap is worth, for any fighter's own agility (#160).
/// Because [`step_distance`] is always positive and this multiplier is
/// `> 1.0`, a leap is strictly larger than that same fighter's own step
/// from every legal starting position.
pub const LEAP_STEP_MULTIPLIER: f32 = 2.0;

/// Ground `StepForward`/`StepBack` cover for a fighter with `agilitate`
/// points of agility: `100 + 10 * agilitate` world units (#160). A higher-
/// agility fighter covers strictly more ground with the same action.
pub const fn step_distance(agilitate: u32) -> f32 {
    STEP_DISTANCE_BASE + STEP_DISTANCE_PER_AGILITATE * agilitate as f32
}

/// Ground `LeapForward` covers for a fighter with `agilitate` points of
/// agility — always [`LEAP_STEP_MULTIPLIER`] times that fighter's own
/// [`step_distance`] (#160).
pub const fn leap_distance(agilitate: u32) -> f32 {
    LEAP_STEP_MULTIPLIER * step_distance(agilitate)
}

/// Ground covered by `StepForward`/`StepBack` at the baseline `agilitate`
/// of 1 — every roster hero and enemy's starting agility, and the value
/// position/arena/render fixtures that don't model a specific fighter's
/// agility call `advance`/`retreat` with directly. Equals
/// [`step_distance`]`(1)`; unchanged from the pre-#160 fixed constant.
pub const STEP_DISTANCE: f32 = step_distance(1);

/// Ground covered by `LeapForward` at the baseline `agilitate` of 1.
/// Equals [`leap_distance`]`(1)`; unchanged from the pre-#160 fixed
/// constant.
pub const LEAP_DISTANCE: f32 = leap_distance(1);

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
    /// at [`MAX_SEPARATION`] and at the mover's own arena wall — true wall
    /// pinning (#160): only the mover's position changes, so a fighter that
    /// has backed itself into a corner simply stops there instead of
    /// pushing its opponent. Always safe to call (a fully pinned fighter is
    /// left in place, a no-op); [`Self::can_retreat`] is the legality check
    /// consumers use to disable the command before it becomes a no-op.
    pub fn retreat(&mut self, side: CombatSide, distance: f32) {
        let separation = (self.separation() + distance).min(MAX_SEPARATION);
        let mut next = *self;
        next.place_at_separation(side, separation);
        match side {
            CombatSide::Player => next.player_x = next.player_x.max(ARENA_BOUNDS.min_x),
            CombatSide::Enemy => next.enemy_x = next.enemy_x.min(ARENA_BOUNDS.max_x),
        }
        *self = next;
    }

    /// Whether `side` is pinned against its own arena wall: zero room left
    /// to retreat regardless of the [`MAX_SEPARATION`] cap.
    pub fn is_wall_pinned(&self, side: CombatSide) -> bool {
        match side {
            CombatSide::Player => self.player_x <= ARENA_BOUNDS.min_x,
            CombatSide::Enemy => self.enemy_x >= ARENA_BOUNDS.max_x,
        }
    }

    /// Whether `side` can legally retreat at all right now — positive
    /// [`Self::retreat_space`], i.e. under [`MAX_SEPARATION`] and not
    /// [`Self::is_wall_pinned`]. `retreat` itself always clamps safely, but
    /// zero room is not a smaller retreat, it is no retreat:
    /// [`super::actions::action_disabled_reason`] reads this to reject the
    /// command and expose a player-readable reason instead of letting it
    /// silently do nothing.
    pub fn can_retreat(&self, side: CombatSide) -> bool {
        self.retreat_space(side) > 0.0
    }

    /// World units of ground `side` can still open by retreating — the
    /// tighter of the [`MAX_SEPARATION`] headroom and the room to `side`'s
    /// own arena wall, never negative. The quantitative form of
    /// [`Self::can_retreat`] the AI reasons over (#160): how much escape
    /// room a fighter has left before the corner takes retreat away.
    pub fn retreat_space(&self, side: CombatSide) -> f32 {
        let cap_room = MAX_SEPARATION - self.separation();
        let wall_room = match side {
            CombatSide::Player => self.player_x - ARENA_BOUNDS.min_x,
            CombatSide::Enemy => ARENA_BOUNDS.max_x - self.enemy_x,
        };
        cap_room.min(wall_room).max(0.0)
    }

    /// Displaces `side`'s fighter by a signed world-unit `delta` (positive
    /// moves it toward `+x`), clamped to the arena's own walls and to the
    /// no-pass-through minimum separation from the standing opponent — which
    /// never moves. The one shared clamp later shove/recoil integrations
    /// (#160 handoff) build on, unlike [`Self::advance`]/[`Self::retreat`]
    /// this applies no [`MAX_SEPARATION`] cap: an involuntary displacement
    /// is not the same tactical choice as a voluntary retreat.
    pub fn displace_target(&mut self, side: CombatSide, delta: f32) {
        let (min_x, max_x) = match side {
            CombatSide::Player => (ARENA_BOUNDS.min_x, self.enemy_x - MIN_SEPARATION),
            CombatSide::Enemy => (self.player_x + MIN_SEPARATION, ARENA_BOUNDS.max_x),
        };
        let x = match side {
            CombatSide::Player => self.player_x + delta,
            CombatSide::Enemy => self.enemy_x + delta,
        }
        .clamp(min_x, max_x);
        match side {
            CombatSide::Player => self.player_x = x,
            CombatSide::Enemy => self.enemy_x = x,
        }
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
        // From the old FAR equivalent, a leap lands exactly toe-to-toe. The
        // enemy retreats (220 units of room to its own wall, unlike the
        // player's 120) so this reaches MAX_SEPARATION without wall-pinning.
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        assert_eq!(positions.separation(), MAX_SEPARATION);
        positions.advance(CombatSide::Player, LEAP_DISTANCE);
        assert_eq!(positions.separation(), MIN_SEPARATION);
        assert!(positions.in_melee_reach());

        // From the old NEAR equivalent, the same leap clamps to the same
        // toe-to-toe stop a single step reaches — the old band saturation.
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.advance(CombatSide::Player, LEAP_DISTANCE);
        assert_eq!(positions.separation(), MIN_SEPARATION);
    }

    #[test]
    fn retreat_saturates_at_the_maximum_separation_away_from_a_wall() {
        // Shifted 30 units left of the canonical opening so the enemy has
        // 250 units of room to its own wall (rather than exactly 220): two
        // retreats reach MAX_SEPARATION with headroom to spare, no wall
        // pinning involved.
        let mut positions = DuelPositions {
            player_x: -60.0,
            enemy_x: 80.0,
        };
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        assert_eq!(positions.separation(), MAX_SEPARATION);
        assert!(!positions.is_wall_pinned(CombatSide::Enemy));
        let before = positions;
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        assert_eq!(
            positions, before,
            "retreating at max separation holds in place"
        );
    }

    #[test]
    fn a_left_wall_hit_pins_the_mover_alone_the_opponent_never_moves() {
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.player_x, -140.0, "no wall yet");
        // Raw target: -140 - 110 = -250, crossing the -150 wall by 100; only
        // the player clamps there, the enemy is untouched (true wall
        // pinning, #160 — replaces the old pair slide).
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions.player_x, ARENA_BOUNDS.min_x);
        assert_eq!(
            positions.enemy_x, 110.0,
            "the standing opponent never moves"
        );
        assert_eq!(positions.separation(), 260.0, "short of the 360 cap");
        assert!(positions.is_wall_pinned(CombatSide::Player));
        assert!(!positions.can_retreat(CombatSide::Player));

        // A further retreat attempt is a safe no-op.
        let before = positions;
        positions.retreat(CombatSide::Player, STEP_DISTANCE);
        assert_eq!(positions, before, "a pinned retreat changes nothing");
    }

    #[test]
    fn a_right_wall_hit_pins_the_mover_alone_the_opponent_never_moves() {
        // Walk the pair rightwards first: the enemy retreats, the player
        // chases back into reach, leaving the pair at (80, 220).
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.advance(CombatSide::Player, STEP_DISTANCE);
        assert_eq!((positions.player_x, positions.enemy_x), (80.0, 220.0));
        // Raw target separation 360 (already the cap): 80 + 360 = 440,
        // crossing the +330 wall by 110; only the enemy clamps there.
        positions.retreat(CombatSide::Enemy, LEAP_DISTANCE);
        assert_eq!(positions.enemy_x, ARENA_BOUNDS.max_x);
        assert_eq!(
            positions.player_x, 80.0,
            "the standing opponent never moves"
        );
        assert_eq!(positions.separation(), 250.0, "short of the 360 cap");
        assert!(positions.is_wall_pinned(CombatSide::Enemy));
        assert!(!positions.can_retreat(CombatSide::Enemy));
    }

    #[test]
    fn can_retreat_is_true_away_from_walls_and_under_the_cap() {
        let positions = DuelPositions::starting();
        assert!(positions.can_retreat(CombatSide::Player));
        assert!(positions.can_retreat(CombatSide::Enemy));
    }

    #[test]
    fn can_retreat_is_false_once_separation_saturates_even_off_a_wall() {
        // Shifted 30 units left of the canonical opening (see
        // `retreat_saturates_at_the_maximum_separation_away_from_a_wall`)
        // so this reaches MAX_SEPARATION with headroom to spare, no wall
        // involved.
        let mut positions = DuelPositions {
            player_x: -60.0,
            enemy_x: 80.0,
        };
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        assert_eq!(positions.separation(), MAX_SEPARATION);
        assert!(
            !positions.is_wall_pinned(CombatSide::Enemy),
            "not against a wall in this scenario"
        );
        assert!(
            !positions.can_retreat(CombatSide::Enemy),
            "the separation cap alone also disables retreat"
        );
    }

    #[test]
    fn retreat_space_is_the_tighter_of_cap_headroom_and_wall_room() {
        // Opening placement: separation 140 leaves 220 units of cap
        // headroom; the player has 120 units to its wall (the binding
        // constraint), the enemy 220 (cap and wall tie).
        let positions = DuelPositions::starting();
        assert_eq!(positions.retreat_space(CombatSide::Player), 120.0);
        assert_eq!(positions.retreat_space(CombatSide::Enemy), 220.0);

        // Pinned at the wall: zero space, and can_retreat agrees.
        let mut positions = DuelPositions::starting();
        positions.retreat(CombatSide::Player, 200.0);
        assert!(positions.is_wall_pinned(CombatSide::Player));
        assert_eq!(positions.retreat_space(CombatSide::Player), 0.0);
        assert!(!positions.can_retreat(CombatSide::Player));

        // Saturated at MAX_SEPARATION off the wall: also zero space.
        let mut positions = DuelPositions {
            player_x: -60.0,
            enemy_x: 80.0,
        };
        positions.retreat(CombatSide::Enemy, 220.0);
        assert_eq!(positions.separation(), MAX_SEPARATION);
        assert!(!positions.is_wall_pinned(CombatSide::Enemy));
        assert_eq!(positions.retreat_space(CombatSide::Enemy), 0.0);
        assert!(!positions.can_retreat(CombatSide::Enemy));
    }

    #[test]
    fn step_distance_scales_with_agilitate_and_matches_the_baseline_constant() {
        assert_eq!(step_distance(1), STEP_DISTANCE);
        assert_eq!(step_distance(0), 100.0);
        assert_eq!(step_distance(1), 110.0);
        assert_eq!(step_distance(5), 150.0);
        assert!(
            step_distance(5) > step_distance(1),
            "a more agile fighter covers more ground with the same step"
        );
    }

    #[test]
    fn leap_distance_matches_the_baseline_constant_and_is_strictly_larger_than_step() {
        assert_eq!(leap_distance(1), LEAP_DISTANCE);
        for agilitate in [0, 1, 2, 5, 10, 30] {
            assert!(
                leap_distance(agilitate) > step_distance(agilitate),
                "agilitate {agilitate}: leap must strictly exceed step"
            );
        }
    }

    #[test]
    fn displace_target_clamps_to_the_arena_wall_and_never_crosses_the_opponent() {
        let mut positions = DuelPositions::starting();
        // A huge negative displacement clamps the player at the left wall.
        positions.displace_target(CombatSide::Player, -10_000.0);
        assert_eq!(positions.player_x, ARENA_BOUNDS.min_x);
        assert_eq!(positions.enemy_x, 110.0, "the enemy never moves");

        let mut positions = DuelPositions::starting();
        // A huge positive displacement clamps the enemy at the right wall.
        positions.displace_target(CombatSide::Enemy, 10_000.0);
        assert_eq!(positions.enemy_x, ARENA_BOUNDS.max_x);
        assert_eq!(positions.player_x, -30.0, "the player never moves");

        let mut positions = DuelPositions::starting();
        // The enemy retreats first to open some room, then a huge positive
        // displacement of the player clamps at the no-pass-through minimum
        // separation from the standing enemy, short of the arena wall.
        positions.retreat(CombatSide::Enemy, STEP_DISTANCE);
        positions.displace_target(CombatSide::Player, 10_000.0);
        assert_eq!(positions.player_x, positions.enemy_x - MIN_SEPARATION);
        assert_eq!(positions.separation(), MIN_SEPARATION);

        // Unlike retreat, displacement is not capped at MAX_SEPARATION: both
        // fighters pinned at opposite walls exceed it (480 > 360), while
        // both stay strictly inside the arena bounds.
        let mut positions = DuelPositions::starting();
        positions.displace_target(CombatSide::Player, -10_000.0);
        positions.displace_target(CombatSide::Enemy, 10_000.0);
        assert_eq!(positions.separation(), 480.0);
        assert!(positions.separation() > MAX_SEPARATION);
        assert!(ARENA_BOUNDS.contains(positions.player_x));
        assert!(ARENA_BOUNDS.contains(positions.enemy_x));
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
