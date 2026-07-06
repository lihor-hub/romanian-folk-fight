//! Pure derived-stat formulas for fighters.
//!
//! These are plain functions over [`Attributes`] so the combat engine can call
//! them directly, without an ECS `World`. Later issues (equipment, combat)
//! build on these exact formulas.

use super::Attributes;

/// Maximum critical-hit chance in percent.
pub const CRIT_PERCENT_CAP: i32 = 50;

/// Lower bound of the chance to hit in percent.
pub const HIT_PERCENT_MIN: i32 = 40;

/// Upper bound of the chance to hit in percent.
pub const HIT_PERCENT_MAX: i32 = 95;

/// Maximum hit points: `50 + 10 * vitalitate`.
pub fn max_hp(attrs: &Attributes) -> i32 {
    50 + 10 * attrs.vitalitate as i32
}

/// Maximum stamina: `30 + 5 * vitalitate`.
pub fn max_stamina(attrs: &Attributes) -> i32 {
    30 + 5 * attrs.vitalitate as i32
}

/// Base damage dealt before modifiers: `2 + putere`.
pub fn base_damage(attrs: &Attributes) -> i32 {
    2 + attrs.putere as i32
}

/// Critical-hit chance in percent: `5 + 2 * noroc`, capped at 50.
pub fn crit_percent(attrs: &Attributes) -> i32 {
    (5 + 2 * attrs.noroc as i32).min(CRIT_PERCENT_CAP)
}

/// Chance to hit in percent: `base + 3 * (attacker agilitate - defender
/// agilitate)`, clamped to `[40, 95]`.
pub fn hit_percent(attacker: &Attributes, defender: &Attributes, base: i32) -> i32 {
    (base + 3 * (attacker.agilitate as i32 - defender.agilitate as i32))
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
    fn hit_percent_follows_formula_and_clamps() {
        // (attacker agilitate, defender agilitate, base, expected)
        let cases = [
            // equal agility -> base unchanged
            (1, 1, 80, 80),
            // attacker advantage
            (5, 1, 80, 92),
            // defender advantage
            (1, 5, 80, 68),
            // upper clamp edge: 80 + 3 * 5 = 95 stays
            (6, 1, 80, 95),
            // above upper clamp
            (20, 1, 80, 95),
            // lower clamp edge: 80 + 3 * (-13) = 41 stays... use exact 40
            (1, 14, 79, 40),
            // below lower clamp
            (1, 30, 80, 40),
        ];
        for (atk, def, base, expected) in cases {
            assert_eq!(
                hit_percent(&attrs(1, atk, 1, 1), &attrs(1, def, 1, 1), base),
                expected,
                "attacker {atk}, defender {def}, base {base}"
            );
        }
    }
}
