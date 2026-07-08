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
    pub parts: &'static [CutoutPartKind],
}

impl GearAttachment {
    pub const fn new(parts: &'static [CutoutPartKind]) -> Self {
        Self { parts }
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
    /// Runtime display size in world units for the trimmed gear art.
    pub size: (f32, f32),
    /// Local translation from the owning cutout part origin.
    pub offset: (f32, f32),
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
    size: (f32, f32),
    offset: (f32, f32),
    motion: GearMotion,
    z_offset: f32,
) -> ItemVisual {
    ItemVisual {
        id,
        slot,
        asset_path,
        size,
        offset,
        animated_asset_path: None,
        attachment: attachment_for_slot(slot),
        motion,
        z_offset,
    }
}

const WEAPON_ATTACHMENT: [CutoutPartKind; 1] = [CutoutPartKind::HandFront];
const SHIELD_ATTACHMENT: [CutoutPartKind; 1] = [CutoutPartKind::ForearmBack];
const TORSO_ATTACHMENT: [CutoutPartKind; 1] = [CutoutPartKind::Torso];
const HEAD_ATTACHMENT: [CutoutPartKind; 1] = [CutoutPartKind::Head];
const FEET_ATTACHMENT: [CutoutPartKind; 2] = [CutoutPartKind::FootBack, CutoutPartKind::FootFront];

pub const fn attachment_for_slot(slot: Slot) -> GearAttachment {
    match slot {
        Slot::Weapon => GearAttachment::new(&WEAPON_ATTACHMENT),
        Slot::Shield => GearAttachment::new(&SHIELD_ATTACHMENT),
        Slot::Torso => GearAttachment::new(&TORSO_ATTACHMENT),
        Slot::Head => GearAttachment::new(&HEAD_ATTACHMENT),
        Slot::Feet => GearAttachment::new(&FEET_ATTACHMENT),
    }
}

/// Every starter item currently has a simple self-generated overlay. The
/// order matches [`ItemId::ALL`] and the catalog discriminants.
pub static ITEM_VISUALS: [ItemVisual; 13] = [
    visual(
        ItemId::BataCiobaneasca,
        Slot::Weapon,
        "fighters/gear/runtime/bata_ciobaneasca.png",
        (18.0, 136.0),
        (-2.0, 42.0),
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::ToporDePadurar,
        Slot::Weapon,
        "fighters/gear/runtime/topor_de_padurar.png",
        (42.0, 82.0),
        (8.0, 20.0),
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::Palos,
        Slot::Weapon,
        "fighters/gear/runtime/palos.png",
        (28.0, 92.0),
        (8.0, 20.0),
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::BuzduganCuTreiPeceti,
        Slot::Weapon,
        "gear/buzdugan_cu_trei_peceti.png",
        (40.0, 88.0),
        (8.0, 18.0),
        GearMotion::WeaponHand,
        0.06,
    ),
    visual(
        ItemId::ScutDeLemn,
        Slot::Shield,
        "fighters/gear/runtime/scut_de_lemn.png",
        (54.0, 54.0),
        (-4.0, 4.0),
        GearMotion::ShieldArm,
        0.04,
    ),
    visual(
        ItemId::ScutFerecat,
        Slot::Shield,
        "fighters/gear/runtime/scut_ferecat.png",
        (54.0, 54.0),
        (-4.0, 4.0),
        GearMotion::ShieldArm,
        0.04,
    ),
    visual(
        ItemId::IeDescantata,
        Slot::Torso,
        "fighters/gear/runtime/ie_descantata.png",
        (56.0, 76.0),
        (0.0, 2.0),
        GearMotion::Body,
        0.02,
    ),
    visual(
        ItemId::CojocGros,
        Slot::Torso,
        "fighters/gear/runtime/cojoc_gros.png",
        (54.0, 72.0),
        (0.0, 2.0),
        GearMotion::Body,
        0.02,
    ),
    visual(
        ItemId::CamasaDeZale,
        Slot::Torso,
        "fighters/gear/runtime/camasa_de_zale.png",
        (56.0, 74.0),
        (0.0, 2.0),
        GearMotion::Body,
        0.02,
    ),
    visual(
        ItemId::CaciulaDeOaie,
        Slot::Head,
        "fighters/gear/runtime/caciula_de_oaie.png",
        (40.0, 30.0),
        (0.0, 18.0),
        GearMotion::Head,
        0.05,
    ),
    visual(
        ItemId::CoifDeOstean,
        Slot::Head,
        "fighters/gear/runtime/coif_de_ostean.png",
        (38.0, 34.0),
        (0.0, 14.0),
        GearMotion::Head,
        0.05,
    ),
    visual(
        ItemId::OpinciIuti,
        Slot::Feet,
        "fighters/gear/runtime/opinci_iuti.png",
        (30.0, 18.0),
        (0.0, -2.0),
        GearMotion::Feet,
        0.03,
    ),
    visual(
        ItemId::CizmeDeVoinic,
        Slot::Feet,
        "fighters/gear/runtime/cizme_de_voinic.png",
        (28.0, 24.0),
        (0.0, -1.0),
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
    use std::path::Path;

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
            let expected_attachment: &[CutoutPartKind] = match id.item().slot {
                Slot::Weapon => &[CutoutPartKind::HandFront],
                Slot::Shield => &[CutoutPartKind::ForearmBack],
                Slot::Torso => &[CutoutPartKind::Torso],
                Slot::Head => &[CutoutPartKind::Head],
                Slot::Feet => &[CutoutPartKind::FootBack, CutoutPartKind::FootFront],
            };
            assert_eq!(visual.attachment.parts, expected_attachment);
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

    #[test]
    fn production_runtime_gear_assets_are_used_where_source_art_exists() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let expected_runtime_paths = [
            (
                ItemId::BataCiobaneasca,
                "fighters/gear/runtime/bata_ciobaneasca.png",
            ),
            (
                ItemId::ToporDePadurar,
                "fighters/gear/runtime/topor_de_padurar.png",
            ),
            (ItemId::Palos, "fighters/gear/runtime/palos.png"),
            (ItemId::ScutDeLemn, "fighters/gear/runtime/scut_de_lemn.png"),
            (
                ItemId::ScutFerecat,
                "fighters/gear/runtime/scut_ferecat.png",
            ),
            (
                ItemId::IeDescantata,
                "fighters/gear/runtime/ie_descantata.png",
            ),
            (ItemId::CojocGros, "fighters/gear/runtime/cojoc_gros.png"),
            (
                ItemId::CamasaDeZale,
                "fighters/gear/runtime/camasa_de_zale.png",
            ),
            (
                ItemId::CaciulaDeOaie,
                "fighters/gear/runtime/caciula_de_oaie.png",
            ),
            (
                ItemId::CoifDeOstean,
                "fighters/gear/runtime/coif_de_ostean.png",
            ),
            (ItemId::OpinciIuti, "fighters/gear/runtime/opinci_iuti.png"),
            (
                ItemId::CizmeDeVoinic,
                "fighters/gear/runtime/cizme_de_voinic.png",
            ),
        ];

        for (id, expected_path) in expected_runtime_paths {
            let visual = item_visual(id).expect("visual metadata exists");
            assert_eq!(visual.asset_path, expected_path, "{id:?}");
            assert!(
                assets.join(expected_path).is_file(),
                "{id:?} asset missing at {}",
                assets.join(expected_path).display()
            );
        }

        let buzdugan = item_visual(ItemId::BuzduganCuTreiPeceti).expect("visual metadata exists");
        assert_eq!(buzdugan.asset_path, "gear/buzdugan_cu_trei_peceti.png");
    }
}
