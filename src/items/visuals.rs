//! Visual metadata for equipment layers. Combat rules still read only
//! [`crate::items::Equipment`]'s aggregated stat totals; this table tells the
//! arena which optional overlay art to attach for visible gear.

use super::{ItemId, Slot};

/// Static visual layer data for one catalog item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemVisual {
    /// Catalog item id this visual belongs to.
    pub id: ItemId,
    /// Slot repeated from the catalog for integrity checks and layer logic.
    pub slot: Slot,
    /// Path under `assets/` for the transparent overlay image.
    pub asset_path: &'static str,
    /// Child z offset relative to the fighter body.
    pub z_offset: f32,
}

const fn visual(id: ItemId, slot: Slot, asset_path: &'static str, z_offset: f32) -> ItemVisual {
    ItemVisual {
        id,
        slot,
        asset_path,
        z_offset,
    }
}

/// Every starter item currently has a simple self-generated overlay. The
/// order matches [`ItemId::ALL`] and the catalog discriminants.
pub static ITEM_VISUALS: [ItemVisual; 13] = [
    visual(
        ItemId::BataCiobaneasca,
        Slot::Weapon,
        "gear/bata_ciobaneasca.png",
        0.06,
    ),
    visual(
        ItemId::ToporDePadurar,
        Slot::Weapon,
        "gear/topor_de_padurar.png",
        0.06,
    ),
    visual(ItemId::Palos, Slot::Weapon, "gear/palos.png", 0.06),
    visual(
        ItemId::BuzduganCuTreiPeceti,
        Slot::Weapon,
        "gear/buzdugan_cu_trei_peceti.png",
        0.06,
    ),
    visual(
        ItemId::ScutDeLemn,
        Slot::Shield,
        "gear/scut_de_lemn.png",
        0.04,
    ),
    visual(
        ItemId::ScutFerecat,
        Slot::Shield,
        "gear/scut_ferecat.png",
        0.04,
    ),
    visual(
        ItemId::IeDescantata,
        Slot::Torso,
        "gear/ie_descantata.png",
        0.02,
    ),
    visual(ItemId::CojocGros, Slot::Torso, "gear/cojoc_gros.png", 0.02),
    visual(
        ItemId::CamasaDeZale,
        Slot::Torso,
        "gear/camasa_de_zale.png",
        0.02,
    ),
    visual(
        ItemId::CaciulaDeOaie,
        Slot::Head,
        "gear/caciula_de_oaie.png",
        0.05,
    ),
    visual(
        ItemId::CoifDeOstean,
        Slot::Head,
        "gear/coif_de_ostean.png",
        0.05,
    ),
    visual(ItemId::OpinciIuti, Slot::Feet, "gear/opinci_iuti.png", 0.03),
    visual(
        ItemId::CizmeDeVoinic,
        Slot::Feet,
        "gear/cizme_de_voinic.png",
        0.03,
    ),
];

/// Visual metadata for `id`, if that item has visible art.
pub fn item_visual(id: ItemId) -> Option<&'static ItemVisual> {
    ITEM_VISUALS
        .get(id as usize)
        .filter(|visual| visual.id == id)
}
