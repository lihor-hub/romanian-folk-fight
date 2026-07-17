//! Pure derived-stat formulas for fighters.
//!
//! These are plain functions over [`Attributes`] so the combat engine can call
//! them directly, without an ECS `World`. Later issues (equipment, combat)
//! build on these exact formulas.
//!
//! Every allocatable [`crate::character::AttributeKind`] has a hook here (or
//! in the combat engine, for `agilitate`'s initiative): see the derived-hook
//! table on that enum's docs. `taunt_percent` (carismă) and `max_mana`
//! (magie) are the declared hooks the taunt and spell issues consume; they
//! ship with #128 so no attribute is ever spendable without a derived effect.

use super::Attributes;

/// Maximum critical-hit chance in percent.
pub const CRIT_PERCENT_CAP: i32 = 50;

/// Lower bound of the chance to hit in percent.
pub const HIT_PERCENT_MIN: i32 = 40;

/// Upper bound of the chance to hit in percent.
pub const HIT_PERCENT_MAX: i32 = 95;

/// How much each point of `atac`-over-`apărare` differential moves the hit
/// chance, in percent.
pub const HIT_PERCENT_PER_POINT: i32 = 3;

/// Mana granted per point of `magie`. No flat base on purpose: `magie == 0`
/// is a valid non-caster with zero mana, never normalized upward.
pub const MANA_PER_MAGIE: i32 = 10;

/// Flat base of the taunt success chance in percent.
pub const TAUNT_PERCENT_BASE: i32 = 5;

/// Taunt-chance percent added per point of `carismă`.
pub const TAUNT_PERCENT_PER_CARISMA: i32 = 3;

/// Maximum taunt success chance in percent.
pub const TAUNT_PERCENT_CAP: i32 = 60;

/// Maximum hit points: `50 + 10 * vitalitate`.
pub fn max_hp(attrs: &Attributes) -> i32 {
    50 + 10 * attrs.vitalitate as i32
}

/// Maximum stamina: `30 + 5 * vitalitate`.
pub fn max_stamina(attrs: &Attributes) -> i32 {
    30 + 5 * attrs.vitalitate as i32
}

/// Maximum mana: `10 * magie`. Zero for a `magie == 0` non-caster — there is
/// deliberately no flat base (unlike [`max_hp`]/[`max_stamina`]), so a hero
/// who never spends on magie never gains a mana pool. The spell issue
/// consumes this hook.
pub fn max_mana(attrs: &Attributes) -> i32 {
    MANA_PER_MAGIE * attrs.magie as i32
}

/// Base damage dealt before modifiers: `2 + putere`.
pub fn base_damage(attrs: &Attributes) -> i32 {
    2 + attrs.putere as i32
}

/// Critical-hit chance in percent: `5 + 2 * noroc`, capped at 50.
pub fn crit_percent(attrs: &Attributes) -> i32 {
    (5 + 2 * attrs.noroc as i32).min(CRIT_PERCENT_CAP)
}

/// Taunt success chance in percent: `5 + 3 * carismă`, capped at 60. The
/// carismă hook the taunt/shove issue consumes.
pub fn taunt_percent(attrs: &Attributes) -> i32 {
    (TAUNT_PERCENT_BASE + TAUNT_PERCENT_PER_CARISMA * attrs.carisma as i32).min(TAUNT_PERCENT_CAP)
}

/// Chance to hit in percent: `base + 3 * (attacker atac - defender
/// apărare)`, clamped to `[40, 95]`. #128 moved this differential from the
/// agility gap to the attack/defense split; `agilitate` keeps initiative and
/// the continuous-position role (#134).
pub fn hit_percent(attacker: &Attributes, defender: &Attributes, base: i32) -> i32 {
    (base + HIT_PERCENT_PER_POINT * (attacker.atac as i32 - defender.aparare as i32))
        .clamp(HIT_PERCENT_MIN, HIT_PERCENT_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(putere: u32, agilitate: u32, vitalitate: u32, noroc: u32) -> Attributes {
        Attributes {
            putere,
            agilitate,
            vitalitate,
            noroc,
            ..Attributes::default()
        }
    }

    /// An [`Attributes`] with only the hit-roll pair set; everything else at
    /// the base values.
    fn duel_attrs(atac: u32, aparare: u32) -> Attributes {
        Attributes {
            atac,
            aparare,
            ..Attributes::default()
        }
    }

    #[test]
    fn max_hp_follows_formula() {
        let cases = [(1, 60), (5, 100), (10, 150)];
        for (vitalitate, expected) in cases {
            assert_eq!(
                max_hp(&attrs(1, 1, vitalitate, 1)),
                expected,
                "vitalitate {vitalitate}"
            );
        }
    }

    #[test]
    fn max_stamina_follows_formula() {
        let cases = [(1, 35), (5, 55), (10, 80)];
        for (vitalitate, expected) in cases {
            assert_eq!(
                max_stamina(&attrs(1, 1, vitalitate, 1)),
                expected,
                "vitalitate {vitalitate}"
            );
        }
    }

    #[test]
    fn max_mana_is_zero_for_a_non_caster_and_scales_linearly() {
        // magie 0 is a valid non-caster: exactly zero mana, no flat base.
        let cases = [(0, 0), (1, 10), (5, 50), (12, 120)];
        for (magie, expected) in cases {
            let attrs = Attributes {
                magie,
                ..Attributes::default()
            };
            assert_eq!(max_mana(&attrs), expected, "magie {magie}");
        }
    }

    #[test]
    fn base_damage_follows_formula() {
        let cases = [(1, 3), (5, 7), (20, 22)];
        for (putere, expected) in cases {
            assert_eq!(
                base_damage(&attrs(putere, 1, 1, 1)),
                expected,
                "putere {putere}"
            );
        }
    }

    #[test]
    fn crit_percent_follows_formula_and_caps_at_50() {
        let cases = [
            (1, 7),
            (10, 25),
            (22, 49),
            // 5 + 2 * 23 = 51 -> capped
            (23, 50),
            (100, 50),
        ];
        for (noroc, expected) in cases {
            assert_eq!(
                crit_percent(&attrs(1, 1, 1, noroc)),
                expected,
                "noroc {noroc}"
            );
        }
    }

    #[test]
    fn taunt_percent_follows_formula_and_caps_at_60() {
        let cases = [
            (0, 5),
            (1, 8),
            (10, 35),
            (18, 59),
            // 5 + 3 * 19 = 62 -> capped
            (19, 60),
            (100, 60),
        ];
        for (carisma, expected) in cases {
            let attrs = Attributes {
                carisma,
                ..Attributes::default()
            };
            assert_eq!(taunt_percent(&attrs), expected, "carisma {carisma}");
        }
    }

    #[test]
    fn hit_percent_is_a_function_of_atac_versus_aparare_and_clamps() {
        // (attacker atac, defender aparare, base, expected)
        let cases = [
            // equal atac/aparare -> base unchanged
            (1, 1, 80, 80),
            // attacker advantage
            (5, 1, 80, 92),
            // defender advantage
            (1, 5, 80, 68),
            // upper clamp edge: 80 + 3 * 5 = 95 stays
            (6, 1, 80, 95),
            // above upper clamp
            (20, 1, 80, 95),
            // lower clamp edge: 79 + 3 * (-13) = 40 stays
            (1, 14, 79, 40),
            // below lower clamp
            (1, 30, 80, 40),
        ];
        for (atac, aparare, base, expected) in cases {
            assert_eq!(
                hit_percent(&duel_attrs(atac, 1), &duel_attrs(1, aparare), base),
                expected,
                "atac {atac}, aparare {aparare}, base {base}"
            );
        }
    }

    /// The acceptance criterion of #128: a hero with high `atac` and low
    /// `agilitate` hits more often than the reverse — agility no longer
    /// drives the hit roll at all.
    #[test]
    fn agility_no_longer_moves_the_hit_chance_but_atac_does() {
        let high_atac_low_agi = Attributes {
            atac: 8,
            agilitate: 1,
            ..Attributes::default()
        };
        let high_agi_low_atac = Attributes {
            atac: 1,
            agilitate: 8,
            ..Attributes::default()
        };
        let defender = Attributes::default();
        assert!(
            hit_percent(&high_atac_low_agi, &defender, 80)
                > hit_percent(&high_agi_low_atac, &defender, 80),
            "atac must out-hit agility"
        );
        // Pure agility on either side leaves the chance untouched.
        let agile_defender = Attributes {
            agilitate: 30,
            ..Attributes::default()
        };
        assert_eq!(hit_percent(&Attributes::default(), &agile_defender, 80), 80);
    }
}
