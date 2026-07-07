//! The static starter catalog of Romanian folk equipment (#18): every item
//! the shop will sell, as plain compile-time data. Prices are in galbeni.

use super::{Item, Slot};

/// Unique id of every catalog item. The discriminant is the item's index in
/// [`CATALOG`] (pinned by a test), so lookups are infallible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemId {
    // Weapons
    BataCiobaneasca,
    ToporDePadurar,
    Palos,
    BuzduganCuTreiPeceti,
    // Shields
    ScutDeLemn,
    ScutFerecat,
    // Torso
    IeDescantata,
    CojocGros,
    CamasaDeZale,
    // Head
    CaciulaDeOaie,
    CoifDeOstean,
    // Feet
    OpinciIuti,
    CizmeDeVoinic,
}

impl ItemId {
    /// Every catalog id, for integrity tests and catalog-listing UIs.
    pub const ALL: [Self; 13] = [
        Self::BataCiobaneasca,
        Self::ToporDePadurar,
        Self::Palos,
        Self::BuzduganCuTreiPeceti,
        Self::ScutDeLemn,
        Self::ScutFerecat,
        Self::IeDescantata,
        Self::CojocGros,
        Self::CamasaDeZale,
        Self::CaciulaDeOaie,
        Self::CoifDeOstean,
        Self::OpinciIuti,
        Self::CizmeDeVoinic,
    ];

    /// The catalog entry for this id. Infallible: the enum discriminant is
    /// the [`CATALOG`] index, and both have exactly 13 entries.
    pub fn item(self) -> &'static Item {
        &CATALOG[self as usize]
    }
}

/// One catalog entry; weapons carry no armor and armor pieces no damage.
const fn item(
    id: ItemId,
    name: &'static str,
    slot: Slot,
    damage: i32,
    armor: i32,
    price: u32,
) -> Item {
    Item {
        id,
        name,
        slot,
        damage,
        armor,
        price,
    }
}

/// The full starter catalog, in [`ItemId`] discriminant order.
pub static CATALOG: [Item; 13] = [
    item(
        ItemId::BataCiobaneasca,
        "Bâtă ciobănească",
        Slot::Weapon,
        3,
        0,
        20,
    ),
    item(
        ItemId::ToporDePadurar,
        "Topor de pădurar",
        Slot::Weapon,
        6,
        0,
        60,
    ),
    item(ItemId::Palos, "Paloș", Slot::Weapon, 10, 0, 150),
    item(
        ItemId::BuzduganCuTreiPeceti,
        "Buzdugan cu trei peceți",
        Slot::Weapon,
        16,
        0,
        400,
    ),
    item(ItemId::ScutDeLemn, "Scut de lemn", Slot::Shield, 0, 1, 25),
    item(ItemId::ScutFerecat, "Scut ferecat", Slot::Shield, 0, 3, 120),
    item(ItemId::IeDescantata, "Ie descântată", Slot::Torso, 0, 1, 15),
    item(ItemId::CojocGros, "Cojoc gros", Slot::Torso, 0, 2, 50),
    item(
        ItemId::CamasaDeZale,
        "Cămașă de zale",
        Slot::Torso,
        0,
        4,
        200,
    ),
    item(
        ItemId::CaciulaDeOaie,
        "Căciulă de oaie",
        Slot::Head,
        0,
        1,
        10,
    ),
    item(ItemId::CoifDeOstean, "Coif de oștean", Slot::Head, 0, 2, 80),
    item(ItemId::OpinciIuti, "Opinci iuți", Slot::Feet, 0, 1, 15),
    item(
        ItemId::CizmeDeVoinic,
        "Cizme de voinic",
        Slot::Feet,
        0,
        2,
        70,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_resolves_to_its_own_catalog_entry() {
        // Pins the invariant behind `ItemId::item`: discriminant == index.
        for id in ItemId::ALL {
            assert_eq!(id.item().id, id, "{id:?} must sit at its own index");
        }
    }

    #[test]
    fn ids_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.id, b.id, "duplicate id between {} and {}", a.name, b.name);
            }
        }
    }

    #[test]
    fn every_item_has_a_positive_price() {
        for item in &CATALOG {
            assert!(item.price > 0, "{} must cost something", item.name);
        }
    }

    #[test]
    fn every_slot_has_at_least_two_items() {
        for slot in Slot::ALL {
            let count = CATALOG.iter().filter(|item| item.slot == slot).count();
            assert!(count >= 2, "{slot:?} has only {count} item(s)");
        }
    }

    #[test]
    fn weapons_deal_damage_and_armor_pieces_protect() {
        for item in &CATALOG {
            if item.slot == Slot::Weapon {
                assert!(item.damage > 0, "{} is a damageless weapon", item.name);
                assert_eq!(item.armor, 0, "{} is a weapon with armor", item.name);
            } else {
                assert!(item.armor > 0, "{} is an armorless piece", item.name);
                assert_eq!(item.damage, 0, "{} is armor with damage", item.name);
            }
        }
    }

    #[test]
    fn the_starter_set_matches_the_issue_18_stats() {
        // (id, name, slot, damage, armor, price) — the exact spec table.
        let expected = [
            (
                ItemId::BataCiobaneasca,
                "Bâtă ciobănească",
                Slot::Weapon,
                3,
                0,
                20,
            ),
            (
                ItemId::ToporDePadurar,
                "Topor de pădurar",
                Slot::Weapon,
                6,
                0,
                60,
            ),
            (ItemId::Palos, "Paloș", Slot::Weapon, 10, 0, 150),
            (
                ItemId::BuzduganCuTreiPeceti,
                "Buzdugan cu trei peceți",
                Slot::Weapon,
                16,
                0,
                400,
            ),
            (ItemId::ScutDeLemn, "Scut de lemn", Slot::Shield, 0, 1, 25),
            (ItemId::ScutFerecat, "Scut ferecat", Slot::Shield, 0, 3, 120),
            (ItemId::IeDescantata, "Ie descântată", Slot::Torso, 0, 1, 15),
            (ItemId::CojocGros, "Cojoc gros", Slot::Torso, 0, 2, 50),
            (
                ItemId::CamasaDeZale,
                "Cămașă de zale",
                Slot::Torso,
                0,
                4,
                200,
            ),
            (
                ItemId::CaciulaDeOaie,
                "Căciulă de oaie",
                Slot::Head,
                0,
                1,
                10,
            ),
            (ItemId::CoifDeOstean, "Coif de oștean", Slot::Head, 0, 2, 80),
            (ItemId::OpinciIuti, "Opinci iuți", Slot::Feet, 0, 1, 15),
            (
                ItemId::CizmeDeVoinic,
                "Cizme de voinic",
                Slot::Feet,
                0,
                2,
                70,
            ),
        ];
        assert_eq!(CATALOG.len(), expected.len());
        for (entry, (id, name, slot, damage, armor, price)) in CATALOG.iter().zip(expected) {
            assert_eq!(
                entry,
                &super::item(id, name, slot, damage, armor, price),
                "catalog entry for {name}"
            );
        }
    }
}
