//! Pure point-allocation rules for the character creation screen.
//!
//! No ECS systems here: [`CharacterDraft`] is a plain value type (registered
//! as a `Resource` by the plugin) so the allocation invariants — total spent
//! never exceeds [`FREE_POINTS`], every attribute stays at or above
//! [`BASE_VALUE`] — are unit-testable without a `World`.

use bevy::prelude::*;

use crate::character::Attributes;

/// Curated cycling list of Romanian folk hero names. Free-text name entry is
/// out of scope (Bevy UI has no text-input widget, notably in the browser
/// build), so the player cycles through this list with arrows instead.
pub const FOLK_NAMES: &[&str] = &[
    "Făt-Frumos",
    "Greuceanu",
    "Prâslea",
    "Ileana Cosânzeana",
    "Aprodul Purice",
    "Păcală",
];

/// Free attribute points to distribute on top of the base values.
pub const FREE_POINTS: u32 = 10;

/// Every attribute starts at this value and can never drop below it.
pub const BASE_VALUE: u32 = 1;

/// One of the four allocatable attributes; lets the UI address rows and
/// buttons generically instead of per-attribute systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeKind {
    Putere,
    Agilitate,
    Vitalitate,
    Noroc,
}

impl AttributeKind {
    /// All four kinds, in display order.
    pub const ALL: [AttributeKind; 4] = [
        AttributeKind::Putere,
        AttributeKind::Agilitate,
        AttributeKind::Vitalitate,
        AttributeKind::Noroc,
    ];

    /// Romanian display label for the attribute row.
    pub fn label(self) -> &'static str {
        match self {
            AttributeKind::Putere => "Putere",
            AttributeKind::Agilitate => "Agilitate",
            AttributeKind::Vitalitate => "Vitalitate",
            AttributeKind::Noroc => "Noroc",
        }
    }
}

/// The in-progress character build on the creation screen. Fields are private
/// so every mutation goes through methods that uphold the invariants. The
/// default is the fresh draft: first name, base attributes, nothing spent.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterDraft {
    name_index: usize,
    attributes: Attributes,
}

impl CharacterDraft {
    /// The currently selected folk hero name.
    pub fn name(&self) -> &'static str {
        FOLK_NAMES[self.name_index]
    }

    /// Cycles to the next name, wrapping at the end of the list.
    pub fn next_name(&mut self) {
        self.name_index = (self.name_index + 1) % FOLK_NAMES.len();
    }

    /// Cycles to the previous name, wrapping at the start of the list.
    pub fn previous_name(&mut self) {
        self.name_index = (self.name_index + FOLK_NAMES.len() - 1) % FOLK_NAMES.len();
    }

    /// The attributes as allocated so far.
    pub fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// Current value of one attribute.
    pub fn get(&self, kind: AttributeKind) -> u32 {
        match kind {
            AttributeKind::Putere => self.attributes.putere,
            AttributeKind::Agilitate => self.attributes.agilitate,
            AttributeKind::Vitalitate => self.attributes.vitalitate,
            AttributeKind::Noroc => self.attributes.noroc,
        }
    }

    fn get_mut(&mut self, kind: AttributeKind) -> &mut u32 {
        match kind {
            AttributeKind::Putere => &mut self.attributes.putere,
            AttributeKind::Agilitate => &mut self.attributes.agilitate,
            AttributeKind::Vitalitate => &mut self.attributes.vitalitate,
            AttributeKind::Noroc => &mut self.attributes.noroc,
        }
    }

    /// Free points spent so far.
    pub fn points_spent(&self) -> u32 {
        let attrs = &self.attributes;
        (attrs.putere + attrs.agilitate + attrs.vitalitate + attrs.noroc)
            - AttributeKind::ALL.len() as u32 * BASE_VALUE
    }

    /// Free points still available.
    pub fn points_remaining(&self) -> u32 {
        FREE_POINTS - self.points_spent()
    }

    /// Whether any attribute can still be raised (points remain).
    pub fn can_increase(&self) -> bool {
        self.points_remaining() > 0
    }

    /// Whether `kind` can be lowered (it is above the base value).
    pub fn can_decrease(&self, kind: AttributeKind) -> bool {
        self.get(kind) > BASE_VALUE
    }

    /// Spends one point on `kind`. Returns whether the point was spent.
    pub fn increase(&mut self, kind: AttributeKind) -> bool {
        if !self.can_increase() {
            return false;
        }
        *self.get_mut(kind) += 1;
        true
    }

    /// Refunds one point from `kind`. Returns whether a point was refunded.
    pub fn decrease(&mut self, kind: AttributeKind) -> bool {
        if !self.can_decrease(kind) {
            return false;
        }
        *self.get_mut(kind) -= 1;
        true
    }

    /// Confirm is allowed only when all free points are spent.
    pub fn is_complete(&self) -> bool {
        self.points_remaining() == 0
    }

    /// Restores the fresh-draft state: first name, base attributes, all
    /// [`FREE_POINTS`] unspent.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_draft_has_base_attributes_and_all_points_free() {
        let draft = CharacterDraft::default();
        assert_eq!(draft.attributes(), Attributes::default());
        assert_eq!(draft.points_spent(), 0);
        assert_eq!(draft.points_remaining(), FREE_POINTS);
        assert_eq!(draft.name(), FOLK_NAMES[0]);
        assert!(!draft.is_complete());
    }

    #[test]
    fn increase_spends_a_point() {
        let mut draft = CharacterDraft::default();
        assert!(draft.increase(AttributeKind::Putere));
        assert_eq!(draft.get(AttributeKind::Putere), 2);
        assert_eq!(draft.points_remaining(), FREE_POINTS - 1);
    }

    #[test]
    fn cannot_overspend_past_free_points() {
        let mut draft = CharacterDraft::default();
        for _ in 0..FREE_POINTS {
            assert!(draft.increase(AttributeKind::Agilitate));
        }
        assert_eq!(draft.points_remaining(), 0);
        assert!(!draft.can_increase());
        assert!(!draft.increase(AttributeKind::Putere), "no points left");
        assert_eq!(draft.get(AttributeKind::Putere), BASE_VALUE);
        assert_eq!(
            draft.get(AttributeKind::Agilitate),
            BASE_VALUE + FREE_POINTS
        );
    }

    #[test]
    fn cannot_drop_below_base_value() {
        let mut draft = CharacterDraft::default();
        assert!(!draft.can_decrease(AttributeKind::Vitalitate));
        assert!(!draft.decrease(AttributeKind::Vitalitate));
        assert_eq!(draft.get(AttributeKind::Vitalitate), BASE_VALUE);
        assert_eq!(draft.points_remaining(), FREE_POINTS, "nothing refunded");
    }

    #[test]
    fn decrease_refunds_a_point() {
        let mut draft = CharacterDraft::default();
        draft.increase(AttributeKind::Noroc);
        assert!(draft.can_decrease(AttributeKind::Noroc));
        assert!(draft.decrease(AttributeKind::Noroc));
        assert_eq!(draft.get(AttributeKind::Noroc), BASE_VALUE);
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }

    #[test]
    fn complete_only_when_exactly_all_points_spent() {
        let mut draft = CharacterDraft::default();
        for kind in AttributeKind::ALL {
            draft.increase(kind);
            draft.increase(kind);
        }
        // 8 spent so far.
        assert!(!draft.is_complete());
        draft.increase(AttributeKind::Putere);
        assert!(!draft.is_complete(), "9 of 10 spent");
        draft.increase(AttributeKind::Vitalitate);
        assert!(draft.is_complete(), "all 10 spent");
        draft.decrease(AttributeKind::Putere);
        assert!(!draft.is_complete(), "refund drops completeness");
    }

    #[test]
    fn reset_restores_the_fresh_draft() {
        let mut draft = CharacterDraft::default();
        draft.next_name();
        for _ in 0..5 {
            draft.increase(AttributeKind::Putere);
        }
        draft.reset();
        assert_eq!(draft, CharacterDraft::default());
        assert_eq!(draft.points_remaining(), FREE_POINTS);
    }

    #[test]
    fn name_cycles_forward_and_wraps() {
        let mut draft = CharacterDraft::default();
        for expected in FOLK_NAMES.iter().skip(1) {
            draft.next_name();
            assert_eq!(draft.name(), *expected);
        }
        draft.next_name();
        assert_eq!(draft.name(), FOLK_NAMES[0], "wraps to the first name");
    }

    #[test]
    fn name_cycles_backward_and_wraps() {
        let mut draft = CharacterDraft::default();
        draft.previous_name();
        assert_eq!(
            draft.name(),
            FOLK_NAMES[FOLK_NAMES.len() - 1],
            "wraps to the last name"
        );
        draft.previous_name();
        assert_eq!(draft.name(), FOLK_NAMES[FOLK_NAMES.len() - 2]);
    }
}
