//! Visual metadata for equipment layers. Combat rules still read only
//! [`crate::items::Equipment`]'s aggregated stat totals; this table tells the
//! arena which optional overlay art to attach for visible gear.

use crate::cutout::CutoutPartKind;

use super::{ItemId, Slot};

/// Which part of the fighter an equipment visual should follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GearMotion {
    /// Hand-held weapons: stronger motion on attack and footwork clips.
    WeaponHand,
    /// Guard-side shield: attached to the arm, steadier than the weapon.
    ShieldArm,
    /// Torso armor that follows the body's overall pose.
    Body,
    /// Headwear that follows the head/upper-body bob.
    Head,
    /// Footwear that follows the lower-body footwork.
    Feet,
}

/// Body-part attachment point for one visible equipment layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GearAttachment {
    pub part: CutoutPartKind,
}

impl GearAttachment {
    pub const fn new(part: CutoutPartKind) -> Self {
        Self { part }
    }
}

/// Static visual layer data for one catalog item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemVisual {
    /// Catalog item id this visual belongs to.
    pub id: ItemId,
    /// Slot repeated from the catalog for integrity checks and layer logic.
    pub slot: Slot,
    /// Path under `assets/` for the transparent overlay image.
    pub asset_path: &'static str,
    /// Optional animated replacement sheet. Missing art falls back to
    /// [`Self::asset_path`].
    pub animated_asset_path: Option<&'static str>,
    /// Rig body part this visual should be parented under.
    pub attachment: GearAttachment,
    /// Motion profile used by the arena attachment synchronizer.
    pub motion: GearMotion,
    /// Child z offset relative to the fighter body.
    pub z_offset: f32,
}

impl ItemVisual {
    /// Best image path for an animated layer. Static placeholder art remains
    /// the fallback until per-item animated sheets exist.
    pub fn fallback_asset_path(self) -> &'static str {
        self.animated_asset_path.unwrap_or(self.asset_path)
    }
}

const fn visual(
    id: ItemId,
    slot: Slot,
    asset_path: &'static str,
    motion: GearMotion,
    z_offset: f32,
) -> ItemVisual {
    ItemVisual {
        id,
        slot,
        asset_path,
        animated_asset_path: None,
        attachment: attachment_for_slot(slot),
        motion,
        z_offset,
    }
}

pub const fn attachment_for_slot(slot: Slot) -> GearAttachment {
    match slot {
        Slot::Weapon => GearAttachment::new(CutoutPartKind::HandFront),
        Slot::Shield => GearAttachment::new(CutoutPartKind::ForearmBack),
        Slot::Torso => GearAttachment::new(CutoutPartKind::Torso),
        Slot::Head => GearAttachment::new(CutoutPartKind::Head),
        Slot::Feet => GearAttachment::new(CutoutPartKind::FootFront),
    }
}

/// Every starter item currently has a simple self-generated overlay. The
/// order matches [`ItemId::ALL`] and the catalog discriminants.
pub static ITEM_VISUALS: [ItemVisual; 13] = [
    visual(
        ItemId::BataCiobaneasca,
        Slot::Weapon,
        "gear/bata_ciobaneasca.png",
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::ToporDePadurar,
        Slot::Weapon,
        "gear/topor_de_padurar.png",
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::Palos,
        Slot::Weapon,
        "gear/palos.png",
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::BuzduganCuTreiPeceti,
        Slot::Weapon,
        "gear/buzdugan_cu_trei_peceti.png",
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::ScutDeLemn,
        Slot::Shield,
        "gear/scut_de_lemn.png",
        GearMotion::ShieldArm,
        0.04,
    ),
    visual(
        ItemId::ScutFerecat,
        Slot::Shield,
        "gear/scut_ferecat.png",
        GearMotion::ShieldArm,
        0.04,
    ),
    visual(
        ItemId::IeDescantata,
        Slot::Torso,
        "gear/ie_descantata.png",
        GearMotion::Body,
        0.02,
    ),
    visual(
        ItemId::CojocGros,
        Slot::Torso,
        "gear/cojoc_gros.png",
        GearMotion::Body,
        0.02,
    ),
    visual(
        ItemId::CamasaDeZale,
        Slot::Torso,
        "gear/camasa_de_zale.png",
        GearMotion::Body,
        0.02,
    ),
    visual(
        ItemId::CaciulaDeOaie,
        Slot::Head,
        "gear/caciula_de_oaie.png",
        GearMotion::Head,
        0.05,
    ),
    visual(
        ItemId::CoifDeOstean,
        Slot::Head,
        "gear/coif_de_ostean.png",
        GearMotion::Head,
        0.05,
    ),
    visual(
        ItemId::OpinciIuti,
        Slot::Feet,
        "gear/opinci_iuti.png",
        GearMotion::Feet,
        0.03,
    ),
    visual(
        ItemId::CizmeDeVoinic,
        Slot::Feet,
        "gear/cizme_de_voinic.png",
        GearMotion::Feet,
        0.03,
    ),
];

/// Visual metadata for `id`, if that item has visible art.
pub fn item_visual(id: ItemId) -> Option<&'static ItemVisual> {
    ITEM_VISUALS
        .get(id as usize)
        .filter(|visual| visual.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_metadata_matches_catalog_slots_and_motion() {
        for id in ItemId::ALL {
            let visual = item_visual(id).expect("visual metadata exists");
            assert_eq!(visual.id, id);
            assert_eq!(visual.slot, id.item().slot);
            let expected_motion = match id.item().slot {
                Slot::Weapon => GearMotion::WeaponHand,
                Slot::Shield => GearMotion::ShieldArm,
                Slot::Torso => GearMotion::Body,
                Slot::Head => GearMotion::Head,
                Slot::Feet => GearMotion::Feet,
            };
            assert_eq!(visual.motion, expected_motion);
            let expected_attachment = match id.item().slot {
                Slot::Weapon => CutoutPartKind::HandFront,
                Slot::Shield => CutoutPartKind::ForearmBack,
                Slot::Torso => CutoutPartKind::Torso,
                Slot::Head => CutoutPartKind::Head,
                Slot::Feet => CutoutPartKind::FootFront,
            };
            assert_eq!(visual.attachment.part, expected_attachment);
        }
    }

    #[test]
    fn missing_animated_gear_art_falls_back_to_static_asset() {
        for id in ItemId::ALL {
            let visual = item_visual(id).expect("visual metadata exists");
            assert_eq!(visual.animated_asset_path, None);
            assert_eq!(visual.fallback_asset_path(), visual.asset_path);
        }
    }
}
