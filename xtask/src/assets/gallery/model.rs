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

use super::super::aggregate::ResolvedRecord;
use super::super::schema::{Category, Status};

/// Fixed anatomical draw order, back-to-front, matching the literal
/// authoring order of `human_parts()` in `src/cutout.rs` (verified there by
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

/// Every `fighter-runtime-part` record belonging to `identity`, sorted by
/// anatomical draw order (ties broken by id for determinism).
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
        let Some(last) = id_segment(&record.record.id, usize::MAX) else {
            continue;
        };
        let (scene, _layer) = split_scene_layer(last);
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
