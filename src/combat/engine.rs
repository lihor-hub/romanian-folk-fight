//! Pure turn-based combat resolution: no Bevy ECS types beyond the plain
//! [`Attributes`] data struct, so every branch is unit-testable with a seeded
//! RNG. The ECS glue in [`super::systems`] builds [`FighterState`] snapshots
//! from components, calls [`resolve_action`], and writes the results back.

use rand::{Rng, RngExt as _};

use crate::character::{Attributes, stats};

/// Stamina cost of [`CombatAction::QuickStrike`].
pub const QUICK_STRIKE_COST: i32 = 5;
/// Stamina cost of [`CombatAction::HeavyStrike`].
pub const HEAVY_STRIKE_COST: i32 = 15;
/// Stamina cost of [`CombatAction::Block`].
pub const BLOCK_COST: i32 = 3;
/// Stamina restored by [`CombatAction::Rest`], capped at max stamina.
pub const REST_RESTORE: i32 = 20;
/// Base hit chance in percent for [`CombatAction::QuickStrike`].
pub const QUICK_STRIKE_BASE_HIT: i32 = 80;
/// Base hit chance in percent for [`CombatAction::HeavyStrike`].
pub const HEAVY_STRIKE_BASE_HIT: i32 = 60;

/// One of the four things a fighter can do on their turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatAction {
    /// Cheap, accurate strike for `base_damage`.
    QuickStrike,
    /// Expensive, inaccurate strike for `2 * base_damage`.
    HeavyStrike,
    /// Raise a guard until the actor's next turn: incoming damage is halved
    /// (rounded down, min 1) and crits are downgraded to normal hits.
    Block,
    /// Recover [`REST_RESTORE`] stamina, capped at max.
    Rest,
}

/// Snapshot of one fighter for the pure resolver, constructed from the ECS
/// `Health`/`Stamina`/`Attributes` components (and the blocking flag tracked
/// by the turn resource).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FighterState {
    /// Current hit points; [`resolve_action`] floors it at 0.
    pub hp: i32,
    /// Current stamina; never driven below 0.
    pub stamina: i32,
    /// The fighter's attributes, source of every derived stat.
    pub attributes: Attributes,
    /// Whether the fighter is guarding since their last turn.
    pub blocking: bool,
}

impl FighterState {
    /// A fighter at full pools (per the #8 formulas) and not blocking.
    pub fn new(attributes: Attributes) -> Self {
        Self {
            hp: stats::max_hp(&attributes),
            stamina: stats::max_stamina(&attributes),
            attributes,
            blocking: false,
        }
    }

    /// Max stamina derived from the attributes; the Rest cap.
    pub fn max_stamina(&self) -> i32 {
        stats::max_stamina(&self.attributes)
    }
}

/// What happened during one action resolution. The HUD log, announcer, and
/// FX issues all consume these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatEvent {
    /// A strike was attempted but missed.
    Missed,
    /// A normal hit landed for `dmg`.
    Hit { dmg: i32 },
    /// A critical hit landed for `dmg` (already doubled).
    Crit { dmg: i32 },
    /// The target was guarding: the (possibly crit-downgraded) hit was
    /// halved to `dmg`.
    Blocked { dmg: i32 },
    /// The actor raised a guard.
    Guarded,
    /// The actor rested and recovered `amount` stamina.
    Rested { amount: i32 },
    /// A strike was rejected because the actor lacked the stamina; no state
    /// changed.
    OutOfStamina,
    /// The target's hp reached 0 with this action.
    Defeated,
}

/// Whether the player opens the round: the fighter with higher `agilitate`
/// acts first, and ties go to the player.
pub fn player_acts_first(player: &Attributes, enemy: &Attributes) -> bool {
    player.agilitate >= enemy.agilitate
}

/// Resolves one action of `actor` against `target`, mutating both states and
/// returning the events that occurred, in order. Deterministic for a given
/// RNG state.
///
/// The actor's guard from a previous [`CombatAction::Block`] lapses when they
/// execute their next action — a strike rejected for lack of stamina is a
/// true no-op and leaves the guard up.
pub fn resolve_action(
    actor: &mut FighterState,
    target: &mut FighterState,
    action: CombatAction,
    rng: &mut impl Rng,
) -> Vec<CombatEvent> {
    match action {
        CombatAction::QuickStrike => strike(
            actor,
            target,
            QUICK_STRIKE_COST,
            QUICK_STRIKE_BASE_HIT,
            1,
            rng,
        ),
        CombatAction::HeavyStrike => strike(
            actor,
            target,
            HEAVY_STRIKE_COST,
            HEAVY_STRIKE_BASE_HIT,
            2,
            rng,
        ),
        CombatAction::Block => {
            // Blocking is always available; the cost saturates at 0 so a
            // spent fighter is never left without a defensive option.
            actor.stamina = (actor.stamina - BLOCK_COST).max(0);
            actor.blocking = true;
            vec![CombatEvent::Guarded]
        }
        CombatAction::Rest => {
            actor.blocking = false;
            let amount = REST_RESTORE.min(actor.max_stamina() - actor.stamina);
            actor.stamina += amount;
            vec![CombatEvent::Rested { amount }]
        }
    }
}

/// Resolves one strike: pay `cost` stamina (or reject the strike as a
/// no-op), roll to hit against `base_hit`, roll to crit, then apply the
/// target's guard before dealing damage.
fn strike(
    actor: &mut FighterState,
    target: &mut FighterState,
    cost: i32,
    base_hit: i32,
    damage_multiplier: i32,
    rng: &mut impl Rng,
) -> Vec<CombatEvent> {
    if actor.stamina < cost {
        return vec![CombatEvent::OutOfStamina];
    }
    actor.blocking = false;
    actor.stamina -= cost;

    let hit_chance = stats::hit_percent(&actor.attributes, &target.attributes, base_hit);
    if !roll(rng, hit_chance) {
        return vec![CombatEvent::Missed];
    }

    let base = damage_multiplier * stats::base_damage(&actor.attributes);
    let crit = roll(rng, stats::crit_percent(&actor.attributes));
    let (dmg, event) = if target.blocking {
        // A guard downgrades crits to normal hits, then halves the damage
        // (rounded down, min 1).
        let dmg = (base / 2).max(1);
        (dmg, CombatEvent::Blocked { dmg })
    } else if crit {
        let dmg = 2 * base;
        (dmg, CombatEvent::Crit { dmg })
    } else {
        (base, CombatEvent::Hit { dmg: base })
    };

    target.hp = (target.hp - dmg).max(0);
    let mut events = vec![event];
    if target.hp == 0 {
        events.push(CombatEvent::Defeated);
    }
    events
}

/// One `0..100` roll against a percent chance.
fn roll(rng: &mut impl Rng, percent: i32) -> bool {
    rng.random_range(0..100) < percent
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::stats::{CRIT_PERCENT_CAP, HIT_PERCENT_MAX, HIT_PERCENT_MIN};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn attrs(putere: u32, agilitate: u32, vitalitate: u32, noroc: u32) -> Attributes {
        Attributes {
            putere,
            agilitate,
            vitalitate,
            noroc,
        }
    }

    /// Standard test fighter: putere 4 (base damage 6), vitalitate 5
    /// (100 hp, 55 stamina).
    fn fighter() -> FighterState {
        FighterState::new(attrs(4, 2, 5, 1))
    }

    fn roll(rng: &mut ChaCha8Rng) -> i32 {
        rng.random_range(0..100)
    }

    /// Finds a seed whose successive `0..100` rolls satisfy `pred`, so test
    /// outcomes are forced regardless of the exact (clamped) percentages.
    fn rng_where(pred: impl Fn(&mut ChaCha8Rng) -> bool) -> ChaCha8Rng {
        for seed in 0..100_000u64 {
            let mut probe = ChaCha8Rng::seed_from_u64(seed);
            if pred(&mut probe) {
                return ChaCha8Rng::seed_from_u64(seed);
            }
        }
        panic!("no seed under 100000 produced the wanted rolls");
    }

    /// First roll hits even at the minimum hit chance; second roll never
    /// crits even at the crit cap.
    fn rng_hit_no_crit() -> ChaCha8Rng {
        rng_where(|r| roll(r) < HIT_PERCENT_MIN && roll(r) >= CRIT_PERCENT_CAP)
    }

    /// First roll hits even at the minimum hit chance; second roll crits
    /// even at the minimum crit chance (5 + 2 * noroc, noroc >= 0).
    fn rng_hit_crit() -> ChaCha8Rng {
        rng_where(|r| roll(r) < HIT_PERCENT_MIN && roll(r) < 5)
    }

    /// First roll misses even at the maximum hit chance.
    fn rng_miss() -> ChaCha8Rng {
        rng_where(|r| roll(r) >= HIT_PERCENT_MAX)
    }

    #[test]
    fn quick_strike_hit_deals_base_damage_and_costs_five_stamina() {
        let mut actor = fighter();
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Hit { dmg: 6 }]);
        assert_eq!(target.hp, 94, "base_damage(putere 4) = 6");
        assert_eq!(actor.stamina, 50, "quick strike costs 5");
    }

    #[test]
    fn heavy_strike_hit_deals_double_damage_and_costs_fifteen_stamina() {
        let mut actor = fighter();
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::HeavyStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Hit { dmg: 12 }]);
        assert_eq!(target.hp, 88, "2 * base_damage(putere 4) = 12");
        assert_eq!(actor.stamina, 40, "heavy strike costs 15");
    }

    #[test]
    fn missed_strike_spends_stamina_but_leaves_the_target_untouched() {
        let mut actor = fighter();
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_miss(),
        );
        assert_eq!(events, vec![CombatEvent::Missed]);
        assert_eq!(target, fighter(), "target untouched on a miss");
        assert_eq!(actor.stamina, 50, "stamina is spent even on a miss");
    }

    #[test]
    fn crit_doubles_quick_strike_damage() {
        let mut actor = fighter();
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Crit { dmg: 12 }]);
        assert_eq!(target.hp, 88);
    }

    #[test]
    fn crit_doubles_heavy_strike_damage() {
        let mut actor = fighter();
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::HeavyStrike,
            &mut rng_hit_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Crit { dmg: 24 }]);
        assert_eq!(target.hp, 76);
    }

    #[test]
    fn block_raises_the_guard_and_costs_three_stamina() {
        let mut actor = fighter();
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::Block,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Guarded]);
        assert!(actor.blocking);
        assert_eq!(actor.stamina, 52, "block costs 3");
        assert_eq!(target, fighter(), "block never touches the target");
    }

    #[test]
    fn block_with_low_stamina_floors_at_zero() {
        let mut actor = fighter();
        actor.stamina = 2;
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::Block,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Guarded]);
        assert!(actor.blocking);
        assert_eq!(actor.stamina, 0, "stamina floors at 0, never negative");
    }

    #[test]
    fn blocked_hit_is_halved_rounded_down() {
        // putere 3 -> base damage 5 -> blocked to 5 / 2 = 2.
        let mut actor = FighterState::new(attrs(3, 2, 5, 1));
        let mut target = fighter();
        target.blocking = true;
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Blocked { dmg: 2 }]);
        assert_eq!(target.hp, 98);
        assert!(target.blocking, "guard holds until the blocker's own turn");
    }

    #[test]
    fn blocked_hit_deals_at_least_one_damage() {
        // putere 0 -> base damage 2 -> blocked to (2 / 2).max(1) = 1.
        let mut actor = FighterState::new(attrs(0, 2, 5, 1));
        let mut target = fighter();
        target.blocking = true;
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Blocked { dmg: 1 }]);
        assert_eq!(target.hp, 99);
    }

    #[test]
    fn block_downgrades_a_crit_to_a_normal_blocked_hit() {
        let mut actor = fighter();
        let mut target = fighter();
        target.blocking = true;
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_crit(),
        );
        assert_eq!(
            events,
            vec![CombatEvent::Blocked { dmg: 3 }],
            "crit is downgraded, then base damage 6 is halved to 3"
        );
        assert_eq!(target.hp, 97);
    }

    #[test]
    fn the_guard_lapses_when_the_blocker_takes_their_next_turn() {
        let mut actor = fighter();
        actor.blocking = true;
        let mut target = fighter();
        resolve_action(
            &mut actor,
            &mut target,
            CombatAction::Rest,
            &mut rng_hit_no_crit(),
        );
        assert!(!actor.blocking, "guard lapses on the actor's next action");
    }

    #[test]
    fn rest_restores_twenty_stamina() {
        let mut actor = fighter();
        actor.stamina = 20;
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::Rest,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Rested { amount: 20 }]);
        assert_eq!(actor.stamina, 40);
    }

    #[test]
    fn rest_never_exceeds_max_stamina() {
        let mut actor = fighter();
        actor.stamina = 45; // max is 55
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::Rest,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Rested { amount: 10 }]);
        assert_eq!(actor.stamina, 55, "capped at max_stamina(vitalitate 5)");
    }

    #[test]
    fn rest_at_full_stamina_restores_nothing() {
        let mut actor = fighter();
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::Rest,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Rested { amount: 0 }]);
        assert_eq!(actor.stamina, 55);
    }

    #[test]
    fn strikes_without_enough_stamina_are_rejected_as_no_ops() {
        for (action, stamina) in [
            (CombatAction::QuickStrike, QUICK_STRIKE_COST - 1),
            (CombatAction::HeavyStrike, HEAVY_STRIKE_COST - 1),
        ] {
            let mut actor = fighter();
            actor.stamina = stamina;
            let mut target = fighter();
            let events = resolve_action(&mut actor, &mut target, action, &mut rng_hit_no_crit());
            assert_eq!(events, vec![CombatEvent::OutOfStamina], "{action:?}");
            assert_eq!(actor.stamina, stamina, "no stamina spent for {action:?}");
            assert_eq!(target, fighter(), "target untouched for {action:?}");
        }
    }

    #[test]
    fn a_rejected_strike_keeps_the_guard_up() {
        let mut actor = fighter();
        actor.blocking = true;
        actor.stamina = QUICK_STRIKE_COST - 1;
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::OutOfStamina]);
        assert!(
            actor.blocking,
            "a rejected strike is a true no-op: the guard stays up"
        );
    }

    #[test]
    fn a_strike_with_exactly_enough_stamina_lands_and_leaves_zero() {
        let mut actor = fighter();
        actor.stamina = QUICK_STRIKE_COST;
        let mut target = fighter();
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(events, vec![CombatEvent::Hit { dmg: 6 }]);
        assert_eq!(actor.stamina, 0);
    }

    #[test]
    fn a_lethal_hit_emits_defeated_and_floors_hp_at_zero() {
        let mut actor = fighter();
        let mut target = fighter();
        target.hp = 5; // incoming 6 damage
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(
            events,
            vec![CombatEvent::Hit { dmg: 6 }, CombatEvent::Defeated]
        );
        assert_eq!(target.hp, 0, "hp floors at 0");
    }

    #[test]
    fn a_blocked_hit_can_still_defeat() {
        let mut actor = fighter();
        let mut target = fighter();
        target.blocking = true;
        target.hp = 3; // incoming blocked damage is 3
        let events = resolve_action(
            &mut actor,
            &mut target,
            CombatAction::QuickStrike,
            &mut rng_hit_no_crit(),
        );
        assert_eq!(
            events,
            vec![CombatEvent::Blocked { dmg: 3 }, CombatEvent::Defeated]
        );
        assert_eq!(target.hp, 0);
    }

    #[test]
    fn resolution_is_deterministic_for_a_fixed_seed() {
        let script = [
            CombatAction::QuickStrike,
            CombatAction::HeavyStrike,
            CombatAction::Block,
            CombatAction::QuickStrike,
            CombatAction::Rest,
            CombatAction::HeavyStrike,
        ];
        let run = || {
            let mut a = fighter();
            let mut b = FighterState::new(attrs(2, 3, 4, 6));
            let mut rng = ChaCha8Rng::seed_from_u64(12);
            let events: Vec<Vec<CombatEvent>> = script
                .iter()
                .map(|&action| resolve_action(&mut a, &mut b, action, &mut rng))
                .collect();
            (a, b, events)
        };
        assert_eq!(run(), run(), "same seed, same duel");
    }

    #[test]
    fn higher_agilitate_acts_first_and_ties_go_to_the_player() {
        let cases = [
            (3, 2, true, "faster player opens"),
            (2, 3, false, "faster enemy opens"),
            (2, 2, true, "tie goes to the player"),
        ];
        for (player_agi, enemy_agi, expected, why) in cases {
            assert_eq!(
                player_acts_first(&attrs(1, player_agi, 1, 1), &attrs(1, enemy_agi, 1, 1)),
                expected,
                "{why}"
            );
        }
    }
}
