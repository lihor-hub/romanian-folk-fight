//! Equipment: item slots, the static folk-item catalog ([`catalog`]), and the
//! [`Equipment`] component that combat reads damage/armor bonuses from.
//!
//! Items are plain static data — the shop issue sells them, the combat engine
//! only ever sees the aggregated `total_damage_bonus()` / `total_armor()`
//! numbers via `FighterState`.

pub mod catalog;
pub mod visuals;

use std::collections::HashMap;

use bevy::prelude::*;

pub use catalog::{CATALOG, ItemId};
pub use visuals::{GearAttachment, GearMotion, ItemVisual, item_visual};

/// Registers the equipment model. Items are plain data for now; shop and
/// drop systems arrive with their own issues.
pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, _app: &mut App) {}
}

/// The five equipment slots of a fighter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Weapon,
    Shield,
    Torso,
    Head,
    Feet,
}

impl Slot {
    /// Every slot, for catalog-coverage checks and slot-iterating UIs.
    pub const ALL: [Self; 5] = [
        Self::Weapon,
        Self::Shield,
        Self::Torso,
        Self::Head,
        Self::Feet,
    ];
}

/// One piece of equipment from the static [`catalog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    /// Unique catalog id; [`Equipment`] stores these, not whole items.
    pub id: ItemId,
    /// Display name (Romanian folk flavor).
    pub name: &'static str,
    /// The slot this item occupies.
    pub slot: Slot,
    /// Flat damage added to every strike's base damage (weapons).
    pub damage: i32,
    /// Flat damage subtracted from incoming hits (armor and shields).
    pub armor: i32,
    /// Shop price in galbeni.
    pub price: u32,
}

/// What a fighter has equipped: at most one item per [`Slot`].
///
/// Fighters spawn with this empty (see `spawn_fighter`), which must behave
/// exactly like the pre-equipment game.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct Equipment {
    slots: HashMap<Slot, ItemId>,
}

impl Equipment {
    /// Equips `id` into its item's slot, returning the item previously
    /// occupying that slot, if any.
    pub fn equip(&mut self, id: ItemId) -> Option<ItemId> {
        self.slots.insert(id.item().slot, id)
    }

    /// The item equipped in `slot`, if any.
    pub fn equipped(&self, slot: Slot) -> Option<ItemId> {
        self.slots.get(&slot).copied()
    }

    /// Sum of the `damage` of every equipped item — the flat bonus added to
    /// the wearer's strikes.
    pub fn total_damage_bonus(&self) -> i32 {
        self.slots.values().map(|id| id.item().damage).sum()
    }

    /// Sum of the `armor` of every equipped item — the flat reduction of
    /// incoming hits.
    pub fn total_armor(&self) -> i32 {
        self.slots.values().map(|id| id.item().armor).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_equipment_grants_no_bonuses() {
        let equipment = Equipment::default();
        assert_eq!(equipment.total_damage_bonus(), 0);
        assert_eq!(equipment.total_armor(), 0);
        for slot in Slot::ALL {
            assert_eq!(equipment.equipped(slot), None, "{slot:?} starts empty");
        }
    }

    #[test]
    fn equip_places_the_item_in_its_own_slot() {
        let mut equipment = Equipment::default();
        assert_eq!(equipment.equip(ItemId::Palos), None);
        assert_eq!(equipment.equipped(Slot::Weapon), Some(ItemId::Palos));
        assert_eq!(equipment.equipped(Slot::Shield), None);
    }

    #[test]
    fn equipping_the_same_slot_replaces_and_returns_the_old_item() {
        let mut equipment = Equipment::default();
        equipment.equip(ItemId::BataCiobaneasca);
        assert_eq!(
            equipment.equip(ItemId::Palos),
            Some(ItemId::BataCiobaneasca)
        );
        assert_eq!(equipment.equipped(Slot::Weapon), Some(ItemId::Palos));
    }

    #[test]
    fn totals_aggregate_across_all_equipped_slots() {
        let mut equipment = Equipment::default();
        equipment.equip(ItemId::Palos); // damage 10
        equipment.equip(ItemId::ScutFerecat); // armor 3
        equipment.equip(ItemId::CamasaDeZale); // armor 4
        equipment.equip(ItemId::CoifDeOstean); // armor 2
        equipment.equip(ItemId::CizmeDeVoinic); // armor 2
        assert_eq!(equipment.total_damage_bonus(), 10);
        assert_eq!(equipment.total_armor(), 11);
    }
}
