//! Enemy action selection: a pure decision policy over [`FighterState`]
//! snapshots, driven by the same seeded RNG as the resolver so a whole duel
//! stays deterministic for a fixed seed.
//!
//! The policy is a short priority ladder (see [`choose_action`]) followed by
//! a weighted random pick tuned by the per-archetype [`AiProfile`].

use bevy::prelude::*;
use rand::{Rng, RngExt as _};

use crate::character::stats;

use super::engine::{
    CombatAction, DuelDistance, FighterState, HEAVY_DAMAGE_MULTIPLIER, HEAVY_STRIKE_COST,
    QUICK_STRIKE_COST, roll,
};

/// Per-archetype tuning knob for the enemy decision policy, attached as a
/// component to enemy fighters. The folklore roster issue tunes it per
/// opponent; until then every enemy uses the [`Default`] of 0.5.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AiProfile {
    /// How eagerly this enemy attacks, in `0.0..=1.0`: raises the
    /// HeavyStrike weight and lowers the Block weight in the weighted pick.
    pub aggression: f32,
}

impl Default for AiProfile {
    fn default() -> Self {
        Self { aggression: 0.5 }
    }
}

/// Percent chance to Rest (vs Block) in the exhausted-and-hurt branch.
const EXHAUSTED_REST_PERCENT: i32 = 70;
/// Baseline weight of the quick strike in the weighted pick; it is always
/// affordable once the forced-Rest branch has passed.
const QUICK_STRIKE_WEIGHT: f32 = 1.0;
/// Scale applied to `1.0 - aggression` for the Block weight.
const BLOCK_WEIGHT_SCALE: f32 = 0.6;

/// Chooses the enemy's action for this turn. Pure and deterministic for a
/// fixed RNG state and inputs.
///
/// Priority ladder, first match wins:
/// 1. Cannot afford any strike (stamina < quick cost): Rest.
/// 2. Low stamina (< heavy cost) and hurt (hp < 30% of max): 70% Rest,
///    30% Block.
/// 3. Foe in kill range (hp within one heavy strike's damage): the
///    strongest affordable strike.
/// 4. Weighted pick — HeavyStrike weight `aggression` (0 if unaffordable),
///    QuickStrike weight 1.0, Block weight `0.6 * (1.0 - aggression)`, Rest
///    weight the missing-stamina fraction (0 at full stamina, so a rested
///    fighter never Rests).
pub fn choose_action(
    me: &FighterState,
    foe: &FighterState,
    profile: &AiProfile,
    rng: &mut impl Rng,
) -> CombatAction {
    choose_action_at_distance(me, foe, profile, DuelDistance::starting(), rng)
}

/// Chooses an action while accounting for the duel's current spacing.
pub fn choose_action_at_distance(
    me: &FighterState,
    foe: &FighterState,
    profile: &AiProfile,
    distance: DuelDistance,
    rng: &mut impl Rng,
) -> CombatAction {
    // 1. Cannot pay for any strike: recover.
    if me.stamina < QUICK_STRIKE_COST {
        return CombatAction::Rest;
    }
    // 2. Out of reach: close the gap instead of wasting a melee strike.
    if !distance.in_melee_reach() {
        return if distance == DuelDistance::FAR {
            CombatAction::LeapForward
        } else {
            CombatAction::StepForward
        };
    }
    // 3. Running dry while badly hurt: mostly recover, sometimes turtle.
    if me.stamina < HEAVY_STRIKE_COST && 10 * me.hp < 3 * stats::max_hp(&me.attributes) {
        return if roll(rng, EXHAUSTED_REST_PERCENT) {
            CombatAction::Rest
        } else {
            CombatAction::Block
        };
    }
    // 4. Foe in kill range: go for the strongest strike we can pay for.
    if foe.hp <= HEAVY_DAMAGE_MULTIPLIER * stats::base_damage(&me.attributes) {
        return if me.stamina >= HEAVY_STRIKE_COST {
            CombatAction::HeavyStrike
        } else {
            CombatAction::QuickStrike
        };
    }
    // 5. Weighted pick, tuned by aggression and fatigue.
    let aggression = profile.aggression.clamp(0.0, 1.0);
    let heavy_weight = if me.stamina >= HEAVY_STRIKE_COST {
        aggression
    } else {
        0.0
    };
    let missing_stamina_fraction = (me.max_stamina() - me.stamina) as f32 / me.max_stamina() as f32;
    let weighted = [
        (CombatAction::HeavyStrike, heavy_weight),
        (CombatAction::QuickStrike, QUICK_STRIKE_WEIGHT),
        (CombatAction::Block, BLOCK_WEIGHT_SCALE * (1.0 - aggression)),
        (CombatAction::Rest, missing_stamina_fraction),
    ];
    let total: f32 = weighted.iter().map(|(_, weight)| weight).sum();
    let mut remaining = rng.random_range(0.0..total);
    for (action, weight) in weighted {
        // A zero-weight action can never match: `remaining` stays >= 0 past
        // every failed comparison, so `remaining < 0.0` is unreachable here.
        if remaining < weight {
            return action;
        }
        remaining -= weight;
    }
    // Float-rounding fallback; the quick strike is always affordable here.
    CombatAction::QuickStrike
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Attributes;
    use crate::combat::engine::{CombatEvent, resolve_action};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn attrs(putere: u32, agilitate: u32, vitalitate: u32, noroc: u32) -> Attributes {
        Attributes {
            putere,
            agilitate,
            vitalitate,
            noroc,
            ..Attributes::default()
        }
    }

    /// Standard test fighter: putere 4 (base damage 6), vitalitate 5
    /// (100 hp, 55 stamina).
    fn fighter() -> FighterState {
        FighterState::new(attrs(4, 2, 5, 1))
    }

    fn rng(seed: u64) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(seed)
    }

    fn profile(aggression: f32) -> AiProfile {
        AiProfile { aggression }
    }

    #[test]
    fn ai_profile_defaults_to_balanced_aggression() {
        assert_eq!(AiProfile::default(), AiProfile { aggression: 0.5 });
    }

    #[test]
    fn rests_when_it_cannot_afford_any_strike() {
        for stamina in 0..QUICK_STRIKE_COST {
            for seed in 0..20 {
                let mut me = fighter();
                me.stamina = stamina;
                let action = choose_action(&me, &fighter(), &profile(1.0), &mut rng(seed));
                assert_eq!(
                    action,
                    CombatAction::Rest,
                    "stamina {stamina}, seed {seed}: below quick cost the only move is Rest"
                );
            }
        }
    }

    #[test]
    fn exhausted_and_hurt_mostly_rests_and_sometimes_blocks() {
        // Stamina below the heavy cost, hp 29 of 100 (< 30% of max).
        let mut me = fighter();
        me.stamina = 10;
        me.hp = 29;
        let foe = fighter();
        let mut rng = rng(7);
        let mut rests = 0;
        let mut blocks = 0;
        for _ in 0..1_000 {
            match choose_action(&me, &foe, &profile(0.5), &mut rng) {
                CombatAction::Rest => rests += 1,
                CombatAction::Block => blocks += 1,
                other => panic!("exhausted-and-hurt branch only rests or blocks, got {other:?}"),
            }
        }
        assert!(blocks > 0, "the 30% Block arm must occur over 1000 samples");
        assert!(
            rests > blocks,
            "Rest (70%) must dominate Block (30%): {rests} rests vs {blocks} blocks"
        );
    }

    #[test]
    fn kill_range_prefers_the_strongest_affordable_strike() {
        // base_damage(putere 4) = 6, so a foe at 12 hp is in kill range.
        let mut foe = fighter();
        foe.hp = HEAVY_DAMAGE_MULTIPLIER * stats::base_damage(&fighter().attributes);
        for seed in 0..20 {
            let action = choose_action(&fighter(), &foe, &profile(0.0), &mut rng(seed));
            assert_eq!(
                action,
                CombatAction::HeavyStrike,
                "seed {seed}: with full stamina the heavy strike finishes the foe"
            );

            let mut tired = fighter();
            tired.stamina = HEAVY_STRIKE_COST - 1;
            let action = choose_action(&tired, &foe, &profile(1.0), &mut rng(seed));
            assert_eq!(
                action,
                CombatAction::QuickStrike,
                "seed {seed}: without the heavy's stamina the quick strike still finishes"
            );
        }
    }

    #[test]
    fn out_of_reach_enemy_advances_before_attacking() {
        for seed in 0..20 {
            let action = choose_action_at_distance(
                &fighter(),
                &fighter(),
                &profile(1.0),
                DuelDistance::NEAR,
                &mut rng(seed),
            );
            assert_eq!(action, CombatAction::StepForward, "seed {seed}: near gap");

            let action = choose_action_at_distance(
                &fighter(),
                &fighter(),
                &profile(1.0),
                DuelDistance::FAR,
                &mut rng(seed),
            );
            assert_eq!(action, CombatAction::LeapForward, "seed {seed}: far gap");
        }
    }

    #[test]
    fn never_strikes_unaffordably_nor_rests_at_full_over_1000_seeded_samples() {
        let mut rng = rng(42);
        for sample in 0..1_000 {
            let mut me = fighter();
            me.stamina = rng.random_range(0..=me.max_stamina());
            me.hp = rng.random_range(1..=me.hp);
            let mut foe = fighter();
            foe.hp = rng.random_range(1..=foe.hp);
            let aggression = rng.random_range(0..=10) as f32 / 10.0;
            let action = choose_action(&me, &foe, &profile(aggression), &mut rng);
            match action {
                CombatAction::QuickStrike => assert!(
                    me.stamina >= QUICK_STRIKE_COST,
                    "sample {sample}: quick strike with stamina {}",
                    me.stamina
                ),
                CombatAction::HeavyStrike => assert!(
                    me.stamina >= HEAVY_STRIKE_COST,
                    "sample {sample}: heavy strike with stamina {}",
                    me.stamina
                ),
                CombatAction::Rest => assert!(
                    me.stamina < me.max_stamina(),
                    "sample {sample}: rested at full stamina"
                ),
                CombatAction::Block => {}
                CombatAction::StepForward | CombatAction::StepBack | CombatAction::LeapForward => {
                    panic!("sample {sample}: close-range policy chose movement")
                }
            }
        }
    }

    #[test]
    fn higher_aggression_means_more_heavies_and_fewer_blocks() {
        // Full pools and a healthy foe, so every choice takes the weighted
        // branch. Same seed for both aggression levels.
        let count = |aggression: f32| {
            let mut rng = rng(99);
            let (mut heavies, mut blocks) = (0, 0);
            for _ in 0..1_000 {
                match choose_action(&fighter(), &fighter(), &profile(aggression), &mut rng) {
                    CombatAction::HeavyStrike => heavies += 1,
                    CombatAction::Block => blocks += 1,
                    _ => {}
                }
            }
            (heavies, blocks)
        };
        let (heavies_max, blocks_max) = count(1.0);
        let (heavies_min, blocks_min) = count(0.0);
        assert!(
            heavies_max > heavies_min,
            "aggression 1.0 must out-heavy 0.0: {heavies_max} vs {heavies_min}"
        );
        assert!(
            blocks_min > blocks_max,
            "aggression 0.0 must out-block 1.0: {blocks_min} vs {blocks_max}"
        );
        assert_eq!(heavies_min, 0, "aggression 0.0 zeroes the heavy weight");
        assert_eq!(blocks_max, 0, "aggression 1.0 zeroes the block weight");
    }

    #[test]
    fn choice_is_deterministic_for_a_fixed_seed_and_inputs() {
        let run = || {
            let mut rng = rng(1234);
            (0..100)
                .map(|_| choose_action(&fighter(), &fighter(), &profile(0.5), &mut rng))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "same seed and inputs, same choices");
    }

    /// The sanity playtest for the acceptance criteria: with both sides
    /// driven by the policy, duels against the placeholder Strigoi are
    /// winnable and losable across seeds.
    #[test]
    fn duels_against_the_strigoi_are_winnable_and_losable() {
        // The arena's standard player build vs the Strigoi (2/2/2/1).
        let player = FighterState::new(attrs(4, 2, 4, 3));
        let strigoi = FighterState::new(attrs(2, 2, 2, 1));
        let (mut wins, mut losses) = (0, 0);
        for seed in 0..500 {
            let mut rng = rng(seed);
            let (mut a, mut b) = (player, strigoi);
            for _ in 0..500 {
                let action = choose_action(&a, &b, &AiProfile::default(), &mut rng);
                if resolve_action(&mut a, &mut b, action, &mut rng).contains(&CombatEvent::Defeated)
                {
                    break;
                }
                let action = choose_action(&b, &a, &AiProfile::default(), &mut rng);
                if resolve_action(&mut b, &mut a, action, &mut rng).contains(&CombatEvent::Defeated)
                {
                    break;
                }
            }
            match (b.hp, a.hp) {
                (0, _) => wins += 1,
                (_, 0) => losses += 1,
                _ => panic!("seed {seed}: duel did not finish in 500 rounds"),
            }
        }
        assert!(wins > 0, "the fight must remain winnable");
        assert!(losses > 0, "the fight must remain losable");
    }
}
