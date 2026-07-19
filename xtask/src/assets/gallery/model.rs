//! Pure grouping/classification logic for the review gallery (#197, a child
//! of #141): which records form a fighter identity, which form a gear
//! composition, which backgrounds belong to the same parallax scene, and the
//! fixed anatomical draw-order convention used to stack composited layers.
//!
//! Every function here reads only the aggregate #167/#185 already build --
//! no hand-maintained manifest, no metadata duplicated from
//! `src/cutout.rs`/`src/items/visuals.rs` beyond the one documented
//! snapshot below (the draw order), matching the existing precedent in
//! `xtask/src/assets/validate/bounds.rs`'s module docs.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::super::aggregate::ResolvedRecord;
use super::super::schema::{Category, Status};

/// Fixed anatomical draw order, back-to-front, matching both the v3 catalog
/// attachment contract and the literal authoring order of `human_parts()` in
/// `src/cutout.rs` (verified there by
/// the `parts are authored in draw order` test: z_offset is non-decreasing
/// in this exact sequence). This is a point-in-time snapshot of a *sequence*
/// (not a numeric value), not cross-referenced against the live source by
/// this module -- the same documented-snapshot risk #167/#185 already
/// accept for `pivot`/`display`. Sidecars carry no z-order field of their
/// own (only `attachment`/`pivot`/`display`/`crop`), so this table is the
/// one piece of domain knowledge the gallery must supply itself rather than
/// invent per-run.
pub const DRAW_ORDER: &[&str] = &[
    "upper_arm_back",
    "forearm_back",
    "hand_back",
    "thigh_back",
    "shin_back",
    "foot_back",
    "torso",
    "hair",
    "head",
    "upper_arm_front",
    "forearm_front",
    "hand_front",
    "thigh_front",
    "shin_front",
    "foot_front",
];

pub struct HumanLookSpec {
    pub slug: &'static str,
    pub title: &'static str,
    pub resolved_ids: [&'static str; 6],
}

pub const HUMAN_LOOKS: &[HumanLookSpec] = &[
    HumanLookSpec {
        slug: "haiduc",
        title: "Haiduc (authored production look)",
        resolved_ids: [
            "human.body.zvelt.v1",
            "human.face.haiduc.v1",
            "human.hair.plete.v1",
            "human.torso.ie_altita.v1",
            "human.legs.itari.v1",
            "human.feet.opinci.v1",
        ],
    },
    HumanLookSpec {
        slug: "cioban",
        title: "Cioban (authored production look)",
        resolved_ids: [
            "human.body.vanjos.v1",
            "human.face.cioban.v1",
            "human.hair.prins.v1",
            "human.torso.camasa_ciobaneasca.v1",
            "human.legs.cioareci.v1",
            "human.feet.opinci.v1",
        ],
    },
    HumanLookSpec {
        slug: "voinic",
        title: "Voinic (authored production look)",
        resolved_ids: [
            "human.body.voinic.v1",
            "human.face.voinic.v1",
            "human.hair.voinic_scurt.v1",
            "human.torso.camasa_voiniceasca.v1",
            "human.legs.cioareci_voinicesti.v1",
            "human.feet.opinci.v1",
        ],
    },
    HumanLookSpec {
        slug: "ucenic-solomonar",
        title: "Ucenic Solomonar (authored production look)",
        resolved_ids: [
            "human.body.ucenic_solomonar.v1",
            "human.face.ucenic_solomonar.v1",
            "human.hair.ucenic_ciuf.v1",
            "human.torso.suman_de_ucenic.v1",
            "human.legs.cioareci_de_ucenic.v1",
            "human.feet.opinci.v1",
        ],
    },
];

#[derive(Deserialize)]
struct GalleryCatalog {
    parts: Vec<GalleryCatalogPart>,
}

#[derive(Deserialize)]
struct GalleryCatalogPart {
    id: String,
    layers: Vec<GalleryCatalogLayer>,
}

#[derive(Deserialize)]
struct GalleryCatalogLayer {
    asset_path: String,
}

pub struct HumanLookComposition<'a> {
    pub spec: &'static HumanLookSpec,
    pub layers: Vec<&'a ResolvedRecord>,
}

/// Resolves the four exact authored semantic selections through the v3 catalog
/// to their registered albedo records. Asset validation owns diagnostics; an
/// invalid document or missing layer yields no misleading partial specimen.
pub fn human_look_compositions<'a>(
    records: &[&'a ResolvedRecord],
    catalog_json: &str,
) -> Vec<HumanLookComposition<'a>> {
    let Ok(catalog) = serde_json::from_str::<GalleryCatalog>(catalog_json) else {
        return Vec::new();
    };
    let by_part = catalog
        .parts
        .iter()
        .map(|part| (part.id.as_str(), part))
        .collect::<BTreeMap<_, _>>();
    let by_path = records
        .iter()
        .map(|record| (record.full_path.to_string_lossy().into_owned(), *record))
        .collect::<BTreeMap<_, _>>();

    HUMAN_LOOKS
        .iter()
        .filter_map(|spec| {
            let mut layers = Vec::with_capacity(DRAW_ORDER.len());
            for stable_id in spec.resolved_ids {
                let part = by_part.get(stable_id)?;
                for layer in &part.layers {
                    layers.push(*by_path.get(&layer.asset_path)?);
                }
            }
            layers.sort_by_key(|record| {
                draw_order_index(record.record.attachment.as_deref().unwrap_or(""))
            });
            (layers.len() == DRAW_ORDER.len()).then_some(HumanLookComposition { spec, layers })
        })
        .collect()
}

/// Index of `attachment_part` in [`DRAW_ORDER`], or one past the end if it
/// is not a recognized anatomical part name (drawn last, defensively, rather
/// than panicking on unexpected data).
pub fn draw_order_index(attachment_part: &str) -> usize {
    DRAW_ORDER
        .iter()
        .position(|p| *p == attachment_part)
        .unwrap_or(DRAW_ORDER.len())
}

/// Splits a (possibly multi-attachment) `attachment` field value, e.g.
/// `"foot_back,foot_front"`, into its individual anatomical part names.
pub fn attachment_parts(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Every distinct fighter identity (e.g. `human`, `strigoi`, `zmeu`) that has
/// at least one `fighter-runtime-part` record, derived from the second
/// dotted segment of each such record's id (`fighters.<identity>.runtime.*`)
/// rather than hardcoded, so a new identity's sidecars are picked up
/// automatically.
pub fn fighter_identities(records: &[&ResolvedRecord]) -> Vec<String> {
    let mut identities: BTreeSet<String> = BTreeSet::new();
    for record in records {
        if record.record.category != Category::FighterRuntimePart {
            continue;
        }
        if let Some(identity) = id_segment(&record.record.id, 1) {
            identities.insert(identity.to_string());
        }
    }
    identities.into_iter().collect()
}

/// Every albedo `fighter-runtime-part` record belonging to `identity`, sorted
/// by anatomical draw order (ties broken by id for determinism). Companion
/// material channels retain their individual gallery pages but are not body
/// layers in the composed rig.
pub fn identity_parts<'a>(
    records: &[&'a ResolvedRecord],
    identity: &str,
) -> Vec<&'a ResolvedRecord> {
    let mut parts: Vec<&ResolvedRecord> = records
        .iter()
        .copied()
        .filter(|r| {
            r.record.category == Category::FighterRuntimePart
                && id_segment(&r.record.id, 1) == Some(identity)
                && !is_material_channel_id(&r.record.id)
                && r.record.id.split('.').count() == 4
        })
        .collect();
    parts.sort_by_key(|r| {
        let order = r
            .record
            .attachment
            .as_deref()
            .map(draw_order_index)
            .unwrap_or(DRAW_ORDER.len());
        (order, r.record.id.clone())
    });
    parts
}

/// Companion technical maps for `identity`. These are reviewed with the same
/// pivot/facing page as their albedo but never enter a fighter composition.
pub fn identity_material_channels<'a>(
    records: &[&'a ResolvedRecord],
    identity: &str,
) -> Vec<&'a ResolvedRecord> {
    let mut channels: Vec<&ResolvedRecord> = records
        .iter()
        .copied()
        .filter(|r| {
            r.record.category == Category::FighterRuntimePart
                && id_segment(&r.record.id, 1) == Some(identity)
                && is_material_channel_id(&r.record.id)
        })
        .collect();
    channels.sort_by(|a, b| a.record.id.cmp(&b.record.id));
    channels
}

fn is_material_channel_id(id: &str) -> bool {
    ["-mask", "-normal", "-shadow"]
        .iter()
        .any(|suffix| id.ends_with(suffix))
}

/// Every gear record that can be composed onto the representative rig: a
/// shared `gear-runtime-part` (`fighters.gear.runtime.*`) or a `gear-overlay`
/// with `status = runtime` (the one gear item, `gear.buzdugan-cu-trei-peceti`,
/// not yet migrated to a cropped rig part -- see `assets/gear/manifest.toml`).
/// Both carry `attachment`/`pivot`/`display`, which is what a composition
/// needs; legacy gear overlays (superseded placeholder art) carry none of
/// that and are intentionally excluded here (they get a plain asset page,
/// not a composition).
pub fn composable_gear<'a>(records: &[&'a ResolvedRecord]) -> Vec<&'a ResolvedRecord> {
    let mut gear: Vec<&ResolvedRecord> = records
        .iter()
        .copied()
        .filter(|r| {
            (r.record.category == Category::GearRuntimePart
                || r.record.category == Category::GearOverlay)
                && r.record.status == Status::Runtime
                && r.record.attachment.is_some()
                && r.record.pivot.is_some()
                && r.record.display.is_some()
                && !is_material_channel_id(&r.record.id)
        })
        .collect();
    gear.sort_by(|a, b| a.record.id.cmp(&b.record.id));
    gear
}

/// One parsed background-scene grouping: `scene` (e.g. `village`) and its
/// layer records ordered far -> near -> foreground.
pub struct BackgroundScene<'a> {
    pub scene: String,
    pub layers: Vec<&'a ResolvedRecord>,
}

const LAYER_ORDER: &[&str] = &["far", "near", "foreground"];

/// Groups every `background` record by scene prefix (the id's last dotted
/// segment, minus its trailing `-<layer>` suffix, e.g.
/// `backgrounds.village-far` -> scene `village`, layer `far`), sorted by
/// scene name with each scene's layers in back-to-front order.
pub fn background_scenes<'a>(records: &[&'a ResolvedRecord]) -> Vec<BackgroundScene<'a>> {
    let mut by_scene: BTreeMap<String, Vec<&ResolvedRecord>> = BTreeMap::new();
    for record in records {
        if record.record.category != Category::Background {
            continue;
        }
        let Some(scene) = background_scene_of(&record.record.id) else {
            continue;
        };
        by_scene.entry(scene.to_string()).or_default().push(record);
    }
    for layers in by_scene.values_mut() {
        layers.sort_by_key(|r| {
            let last = id_segment(&r.record.id, usize::MAX).unwrap_or("");
            let (_, layer) = split_scene_layer(last);
            let order = LAYER_ORDER
                .iter()
                .position(|l| *l == layer)
                .unwrap_or(LAYER_ORDER.len());
            (order, r.record.id.clone())
        });
    }
    by_scene
        .into_iter()
        .map(|(scene, layers)| BackgroundScene { scene, layers })
        .collect()
}

/// Splits `village-far` into (`village`, `far`) by the last `-` that
/// separates the scene name from a recognized layer suffix. Falls back to
/// treating the whole string as the scene with an empty layer if no known
/// layer suffix matches (defensive; never panics on unexpected naming).
fn split_scene_layer(last_segment: &str) -> (&str, &str) {
    for layer in LAYER_ORDER {
        let suffix = format!("-{layer}");
        if let Some(scene) = last_segment.strip_suffix(suffix.as_str()) {
            return (scene, layer);
        }
    }
    (last_segment, "")
}

/// Returns the dotted segment at `index` of an asset id (`0`-based), or the
/// *last* segment when `index == usize::MAX`. Returns `None` if `index` is
/// out of range (never panics on a malformed/short id).
fn id_segment(id: &str, index: usize) -> Option<&str> {
    let segments: Vec<&str> = id.split('.').collect();
    if index == usize::MAX {
        segments.last().copied()
    } else {
        segments.get(index).copied()
    }
}

/// Public wrapper for an id's last dotted segment (e.g.
/// `fighters.gear.runtime.palos` -> `palos`), used by `pages.rs` to build
/// short, readable synthetic composition-page ids.
pub fn last_segment(id: &str) -> &str {
    id_segment(id, usize::MAX).unwrap_or(id)
}

/// Fighter identity a `fighter-runtime-part` id belongs to (the second
/// dotted segment, e.g. `fighters.human.runtime.head` -> `human`), or `None`
/// for an id that doesn't have that shape. Shared by `mod.rs` (page
/// generation) and `changed.rs` (#211's dependency closure) so both derive
/// the identity the exact same way rather than duplicating the segment math.
pub fn identity_of(id: &str) -> Option<&str> {
    id_segment(id, 1)
}

/// The scene name (e.g. `village`) a `background` record id belongs to (its
/// last dotted segment, minus a recognized `-far`/`-near`/`-foreground`
/// layer suffix), or `None` for an id with no dotted segment at all. Shared
/// by [`background_scenes`] and `changed.rs`'s dependency closure.
pub fn background_scene_of(id: &str) -> Option<&str> {
    let last = id_segment(id, usize::MAX)?;
    let (scene, _layer) = split_scene_layer(last);
    Some(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::schema::{Kind, Record};
    use std::path::PathBuf;

    fn record(id: &str, category: Category, status: Status) -> ResolvedRecord {
        ResolvedRecord {
            sidecar: PathBuf::from("assets/manifest.toml"),
            full_path: PathBuf::from("x.png"),
            record: Record {
                id: id.to_string(),
                path: "x.png".to_string(),
                kind: Kind::Image,
                category,
                status,
                provenance: "repo-generated".to_string(),
                license: "CC0 1.0".to_string(),
                generator: None,
                source_sheet: None,
                license_file: None,
                dimensions: Some([64, 64]),
                sampler: None,
                attachment: None,
                pivot: None,
                display: None,
                crop: None,
            },
        }
    }

    #[test]
    fn draw_order_index_recognizes_every_documented_part() {
        for (i, part) in DRAW_ORDER.iter().enumerate() {
            assert_eq!(draw_order_index(part), i);
        }
        assert_eq!(draw_order_index("nonexistent"), DRAW_ORDER.len());
    }

    #[test]
    fn bundled_human_gallery_exposes_every_v3_catalog_attachment_once() {
        let workspace = crate::process::workspace_root();
        let aggregate = crate::assets::aggregate::build(&workspace.join("assets"));
        let records = aggregate.records.iter().collect::<Vec<_>>();
        let human = identity_parts(&records, "human");
        let attachments = human
            .iter()
            .filter_map(|record| record.record.attachment.as_deref())
            .collect::<BTreeSet<_>>();

        assert_eq!(human.len(), DRAW_ORDER.len());
        assert_eq!(attachments, DRAW_ORDER.iter().copied().collect());
    }

    #[test]
    fn bundled_catalog_has_exact_resolvable_layers_for_all_four_looks() {
        let workspace = crate::process::workspace_root();
        let aggregate = crate::assets::aggregate::build(&workspace.join("assets"));
        let catalog: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                workspace.join("assets/fighters/catalog/human-foundation.json"),
            )
            .expect("bundled human catalog is readable"),
        )
        .expect("bundled human catalog is JSON");
        let parts = catalog["parts"].as_array().expect("catalog has parts");
        let registered = aggregate
            .records
            .iter()
            .map(|record| record.full_path.to_string_lossy().into_owned())
            .collect::<BTreeSet<_>>();

        for (composition_id, expected_ids) in [
            (
                "composition.human.haiduc",
                [
                    "human.body.zvelt.v1",
                    "human.face.haiduc.v1",
                    "human.hair.plete.v1",
                    "human.torso.ie_altita.v1",
                    "human.legs.itari.v1",
                    "human.feet.opinci.v1",
                ],
            ),
            (
                "composition.human.cioban",
                [
                    "human.body.vanjos.v1",
                    "human.face.cioban.v1",
                    "human.hair.prins.v1",
                    "human.torso.camasa_ciobaneasca.v1",
                    "human.legs.cioareci.v1",
                    "human.feet.opinci.v1",
                ],
            ),
            (
                "composition.human.voinic",
                [
                    "human.body.voinic.v1",
                    "human.face.voinic.v1",
                    "human.hair.voinic_scurt.v1",
                    "human.torso.camasa_voiniceasca.v1",
                    "human.legs.cioareci_voinicesti.v1",
                    "human.feet.opinci.v1",
                ],
            ),
            (
                "composition.human.ucenic-solomonar",
                [
                    "human.body.ucenic_solomonar.v1",
                    "human.face.ucenic_solomonar.v1",
                    "human.hair.ucenic_ciuf.v1",
                    "human.torso.suman_de_ucenic.v1",
                    "human.legs.cioareci_de_ucenic.v1",
                    "human.feet.opinci.v1",
                ],
            ),
        ] {
            let mut attachments = BTreeSet::new();
            for stable_id in expected_ids {
                let part = parts
                    .iter()
                    .find(|part| part["id"] == stable_id)
                    .unwrap_or_else(|| panic!("{composition_id} is missing {stable_id}"));
                for layer in part["layers"].as_array().expect("part has layers") {
                    let path = layer["asset_path"].as_str().expect("layer has asset path");
                    assert!(registered.contains(path), "unregistered layer {path}");
                    attachments.insert(
                        layer["attachment"]["point"]
                            .as_str()
                            .expect("layer has attachment"),
                    );
                }
            }
            assert_eq!(attachments, DRAW_ORDER.iter().copied().collect());
        }
    }

    #[test]
    fn attachment_parts_splits_multi_attachment_gear() {
        assert_eq!(
            attachment_parts("foot_back,foot_front"),
            vec!["foot_back", "foot_front"]
        );
        assert_eq!(attachment_parts("torso"), vec!["torso"]);
    }

    #[test]
    fn fighter_identities_derives_from_ids_sorted() {
        let human = record(
            "fighters.human.runtime.head",
            Category::FighterRuntimePart,
            Status::Runtime,
        );
        let strigoi = record(
            "fighters.strigoi.runtime.head",
            Category::FighterRuntimePart,
            Status::Runtime,
        );
        let refs = vec![&human, &strigoi];
        assert_eq!(fighter_identities(&refs), vec!["human", "strigoi"]);
    }

    #[test]
    fn identity_parts_sorts_by_anatomical_draw_order() {
        let mut head = record(
            "fighters.human.runtime.head",
            Category::FighterRuntimePart,
            Status::Runtime,
        );
        head.record.attachment = Some("head".to_string());
        let mut torso = record(
            "fighters.human.runtime.torso",
            Category::FighterRuntimePart,
            Status::Runtime,
        );
        torso.record.attachment = Some("torso".to_string());
        let refs = vec![&head, &torso];
        let sorted = identity_parts(&refs, "human");
        assert_eq!(sorted[0].record.id, "fighters.human.runtime.torso");
        assert_eq!(sorted[1].record.id, "fighters.human.runtime.head");
    }

    #[test]
    fn identity_parts_excludes_material_channels_from_the_composed_rig() {
        let mut head = record(
            "fighters.human.runtime.head",
            Category::FighterRuntimePart,
            Status::Runtime,
        );
        head.record.attachment = Some("head".to_string());
        let mut mask = record(
            "fighters.human.runtime.head-mask",
            Category::FighterRuntimePart,
            Status::Runtime,
        );
        mask.record.attachment = Some("head".to_string());
        let refs = vec![&head, &mask];

        let composed = identity_parts(&refs, "human");
        let channels = identity_material_channels(&refs, "human");

        assert_eq!(composed.len(), 1);
        assert_eq!(composed[0].record.id, "fighters.human.runtime.head");
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].record.id, "fighters.human.runtime.head-mask");
    }

    #[test]
    fn composable_gear_excludes_legacy_overlays_without_rig_metadata() {
        let mut runtime_gear = record(
            "gear.buzdugan-cu-trei-peceti",
            Category::GearOverlay,
            Status::Runtime,
        );
        runtime_gear.record.attachment = Some("hand_front".to_string());
        runtime_gear.record.pivot = Some([8.0, 18.0]);
        runtime_gear.record.display = Some([40.0, 88.0]);
        let legacy = record("gear.palos", Category::GearOverlay, Status::Legacy);
        let refs = vec![&runtime_gear, &legacy];
        let composable = composable_gear(&refs);
        assert_eq!(composable.len(), 1);
        assert_eq!(composable[0].record.id, "gear.buzdugan-cu-trei-peceti");
    }

    #[test]
    fn background_scenes_groups_and_orders_layers() {
        let far = record(
            "backgrounds.village-far",
            Category::Background,
            Status::Runtime,
        );
        let near = record(
            "backgrounds.village-near",
            Category::Background,
            Status::Runtime,
        );
        let fg = record(
            "backgrounds.village-foreground",
            Category::Background,
            Status::Runtime,
        );
        let refs = vec![&fg, &far, &near];
        let scenes = background_scenes(&refs);
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0].scene, "village");
        let ids: Vec<&str> = scenes[0]
            .layers
            .iter()
            .map(|r| r.record.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "backgrounds.village-far",
                "backgrounds.village-near",
                "backgrounds.village-foreground"
            ]
        );
    }
}
