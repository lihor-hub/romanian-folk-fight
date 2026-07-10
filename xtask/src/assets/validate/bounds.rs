//! Metadata-bounds validation for rig-attachment records: `crop` rectangle
//! bounds, `pivot` sanity bounds, `display` positivity, and source/display
//! aspect distortion (#185, a child of #141).
//!
//! # Coordinate system (read this before touching the thresholds below)
//!
//! `pivot` and `display` are **not** pixel coordinates inside the record's
//! own image. They are point-in-time snapshots of the rig-space rest-pose
//! values authored in `src/cutout.rs` (`CutoutPart::offset`/`size`) and
//! `src/items/visuals.rs` (`ItemVisual::offset`/`size`): `offset` is "local
//! translation from the owning cutout part['s parent] origin" (its own doc
//! comment), a Bevy `Transform` translation in the rig's coordinate space,
//! not a point inside the sprite's own raster. That's why real values are
//! routinely negative and can exceed the image's own `dimensions` (e.g.
//! `fighters.human.runtime.foot-back` has `dimensions = [63, 35]` and
//! `pivot = [-8.0, -102.0]`) -- verified against `human_parts()` in
//! `src/cutout.rs` while building this check. A literal "pivot must sit
//! inside the image's pixel rectangle" rule would therefore reject nearly
//! every rig part in the repository as a false positive.
//!
//! What *is* meaningfully checkable without inventing semantics the
//! codebase doesn't have:
//! - `display` must be a finite, positive size (a zero/negative/NaN
//!   display size is never valid, regardless of coordinate system).
//! - `pivot` must be finite, and its magnitude must be within a generous,
//!   documented multiple of the part's own `dimensions` -- not because
//!   pivot is pixel-space, but because a hand-authored rig offset for a
//!   part of a given size is never *wildly* larger than that size in this
//!   project's art style. This catches orders-of-magnitude data-entry
//!   errors (an extra zero, a copy-paste from the wrong row) without
//!   constraining legitimate far-reaching attachment offsets.
//! - `crop`, when known (not the literal `"unknown"` -- see `schema.rs`'s
//!   module docs for why that sentinel exists), *is* a pixel rectangle,
//!   but into the *source sheet* it was cropped from, not the record's own
//!   image. It is checked against that source sheet's recorded
//!   `dimensions` when resolvable.
//! - `display`'s aspect ratio vs. `dimensions`'s aspect ratio *is*
//!   meaningfully comparable (both are plain width/height ratios), so
//!   aspect distortion is checked directly.

use std::collections::BTreeMap;

use super::super::aggregate::ResolvedRecord;
use super::super::diagnostics::Diagnostic;

/// A rig-authored `pivot` component's magnitude must not exceed this
/// multiple of `max(dimensions.width, dimensions.height)`. Chosen from the
/// current inventory: the highest observed `|pivot component| / max(dims)`
/// ratio is ~1.63 (`fighters.human.runtime.foot-back`); `6.0` leaves
/// roughly 3.7x headroom above that for legitimate future rig work while
/// still catching gross corruption (e.g. an accidental extra digit).
pub const PIVOT_SANITY_MULTIPLIER: f32 = 6.0;

/// `display`'s aspect ratio may differ from `dimensions`'s aspect ratio by
/// up to this multiplicative factor in either direction (a ratio outside
/// `[1/tolerance, tolerance]` fails). This rig deliberately displays many
/// limbs non-uniformly scaled from their source crop -- e.g. a diagonal
/// source pose displayed as a straight capsule silhouette, per
/// `docs/art-direction.md`'s "chunky, readable silhouette" guidance -- so a
/// tight tolerance would flag intentional art choices. The current
/// inventory's highest *accepted* distortion is 2.27x
/// (`fighters.strigoi.runtime.foot-front`); `3.0` leaves headroom above
/// that. One record currently exceeds even this generous tolerance --
/// see `ASPECT_DISTORTION_KNOWN_FAILURES` below, which documents it as a
/// known failure rather than loosening this constant further.
pub const ASPECT_DISTORTION_TOLERANCE: f32 = 3.0;

/// Pre-existing aspect-distortion outliers accepted as documented known
/// failures instead of silently loosening `ASPECT_DISTORTION_TOLERANCE`
/// for every record. Each entry is `(asset id, reason)`. Remove an entry
/// once its underlying asset is fixed (re-measure the record's `display`
/// against a same-category sibling, or re-crop the source sheet).
pub const ASPECT_DISTORTION_KNOWN_FAILURES: &[(&str, &str)] = &[(
    "fighters.strigoi.runtime.foot-back",
    "4.13x source/display aspect distortion -- the only rig part in the \
     current inventory beyond the 3x tolerance (its sibling foot-front is \
     2.27x). Pre-existing in the #167 sidecar snapshot, not introduced by \
     this change; filed as a candidate follow-up issue rather than \
     silently widening the tolerance to swallow it.",
)];

/// Runs every metadata-bounds check against the aggregate's resolved
/// records.
pub fn check(records: &[ResolvedRecord]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let by_id: BTreeMap<&str, &ResolvedRecord> =
        records.iter().map(|r| (r.record.id.as_str(), r)).collect();

    for record in records {
        check_display(record, &mut diagnostics);
        check_pivot(record, &mut diagnostics);
        check_crop(record, &by_id, &mut diagnostics);
        check_aspect_distortion(record, &mut diagnostics);
    }

    diagnostics
}

fn check_display(record: &ResolvedRecord, diagnostics: &mut Vec<Diagnostic>) {
    let Some(display) = record.record.display else {
        return;
    };
    let valid =
        display[0].is_finite() && display[1].is_finite() && display[0] > 0.0 && display[1] > 0.0;
    if !valid {
        diagnostics.push(Diagnostic::InvalidDisplaySize {
            sidecar: record.sidecar.clone(),
            id: record.record.id.clone(),
            display,
        });
    }
}

fn check_pivot(record: &ResolvedRecord, diagnostics: &mut Vec<Diagnostic>) {
    let (Some(pivot), Some(dimensions)) = (record.record.pivot, record.record.dimensions) else {
        return;
    };
    if !pivot[0].is_finite() || !pivot[1].is_finite() {
        diagnostics.push(Diagnostic::PivotOutOfBounds {
            sidecar: record.sidecar.clone(),
            id: record.record.id.clone(),
            pivot,
            dimensions,
            tolerance: f32::NAN,
        });
        return;
    }
    let max_dim = dimensions[0].max(dimensions[1]) as f32;
    let bound = PIVOT_SANITY_MULTIPLIER * max_dim;
    if pivot[0].abs() > bound || pivot[1].abs() > bound {
        diagnostics.push(Diagnostic::PivotOutOfBounds {
            sidecar: record.sidecar.clone(),
            id: record.record.id.clone(),
            pivot,
            dimensions,
            tolerance: bound,
        });
    }
}

fn check_crop(
    record: &ResolvedRecord,
    by_id: &BTreeMap<&str, &ResolvedRecord>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(crop) = &record.record.crop else {
        return;
    };
    if crop == "unknown" {
        // Documented known limitation from #167 (see schema.rs) -- an
        // honestly-recorded "we never tracked this" sentinel, not a
        // violation.
        return;
    }
    let Some(rect) = parse_rect(crop) else {
        // Malformed shape is already reported by `aggregate::validate_record`
        // (`WrongClassification` on field `crop`); avoid a duplicate
        // diagnostic for the same root cause.
        return;
    };
    let (x, y, w, h) = rect;
    if w <= 0.0 || h <= 0.0 || x < 0.0 || y < 0.0 {
        diagnostics.push(Diagnostic::CropOutOfBounds {
            sidecar: record.sidecar.clone(),
            id: record.record.id.clone(),
            crop: crop.clone(),
            detail: format!(
                "rectangle must have x >= 0, y >= 0, width > 0, height > 0; got x={x}, y={y}, \
                 width={w}, height={h}"
            ),
        });
        return;
    }

    if let Some(source_id) = &record.record.source_sheet
        && let Some(source) = by_id.get(source_id.as_str())
        && let Some([sw, sh]) = source.record.dimensions
        && (x + w > sw as f32 || y + h > sh as f32)
    {
        diagnostics.push(Diagnostic::CropOutOfBounds {
            sidecar: record.sidecar.clone(),
            id: record.record.id.clone(),
            crop: crop.clone(),
            detail: format!(
                "rectangle x={x}, y={y}, width={w}, height={h} exceeds source sheet \
                 `{source_id}`'s recorded dimensions {sw}x{sh}"
            ),
        });
    }
}

fn check_aspect_distortion(record: &ResolvedRecord, diagnostics: &mut Vec<Diagnostic>) {
    let (Some(display), Some(dimensions)) = (record.record.display, record.record.dimensions)
    else {
        return;
    };
    if dimensions[0] == 0 || dimensions[1] == 0 || display[0] <= 0.0 || display[1] <= 0.0 {
        // Already reported by `check_display`/dimension validation.
        return;
    }
    let source_aspect = dimensions[0] as f32 / dimensions[1] as f32;
    let display_aspect = display[0] / display[1];
    let ratio = display_aspect / source_aspect;
    let in_bounds =
        (1.0 / ASPECT_DISTORTION_TOLERANCE..=ASPECT_DISTORTION_TOLERANCE).contains(&ratio);
    if in_bounds {
        return;
    }
    if ASPECT_DISTORTION_KNOWN_FAILURES
        .iter()
        .any(|(id, _)| *id == record.record.id)
    {
        return;
    }
    diagnostics.push(Diagnostic::AspectDistortion {
        sidecar: record.sidecar.clone(),
        id: record.record.id.clone(),
        dimensions,
        display,
        ratio,
        tolerance: ASPECT_DISTORTION_TOLERANCE,
    });
}

/// Parses an `"x,y,w,h"` crop-rectangle string. Shape (four comma-separated
/// numbers) is already enforced by `aggregate::is_crop_rect_shaped`; this
/// just extracts the numbers for the bounds check above.
fn parse_rect(value: &str) -> Option<(f32, f32, f32, f32)> {
    let parts: Vec<f32> = value
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.len() != 4 {
        return None;
    }
    Some((parts[0], parts[1], parts[2], parts[3]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::schema::{Category, Kind, Record, Status};
    use std::path::PathBuf;

    fn rig_record(
        id: &str,
        dimensions: [u32; 2],
        pivot: [f32; 2],
        display: [f32; 2],
        crop: Option<&str>,
        source_sheet: Option<&str>,
    ) -> ResolvedRecord {
        ResolvedRecord {
            sidecar: PathBuf::from("assets/fighters/human/runtime/manifest.toml"),
            full_path: PathBuf::from(format!("fighters/human/runtime/{id}.png")),
            record: Record {
                id: id.to_string(),
                path: format!("{id}.png"),
                kind: Kind::Image,
                category: Category::FighterRuntimePart,
                status: Status::Runtime,
                provenance: "cropped-from-source-sheet".to_string(),
                license: "Same as project assets unless superseded".to_string(),
                generator: None,
                source_sheet: source_sheet.map(str::to_string),
                license_file: None,
                dimensions: Some(dimensions),
                sampler: None,
                attachment: Some(id.to_string()),
                pivot: Some(pivot),
                display: Some(display),
                crop: crop.map(str::to_string),
            },
        }
    }

    fn source_sheet_record(id: &str, dimensions: [u32; 2]) -> ResolvedRecord {
        ResolvedRecord {
            sidecar: PathBuf::from("assets/fighters/human/source/manifest.toml"),
            full_path: PathBuf::from(format!("fighters/human/source/{id}.png")),
            record: Record {
                id: id.to_string(),
                path: format!("{id}.png"),
                kind: Kind::Image,
                category: Category::FighterSourceSheet,
                status: Status::Source,
                provenance: "openai-generated".to_string(),
                license: "project-owned".to_string(),
                generator: None,
                source_sheet: None,
                license_file: None,
                dimensions: Some(dimensions),
                sampler: None,
                attachment: None,
                pivot: None,
                display: None,
                crop: None,
            },
        }
    }

    #[test]
    fn a_realistic_rig_record_with_a_negative_pivot_is_clean() {
        // Real data: fighters.human.runtime.foot-back.
        let record = rig_record(
            "foot-back",
            [63, 35],
            [-8.0, -102.0],
            [28.0, 12.0],
            Some("unknown"),
            None,
        );
        let diagnostics = check(&[record]);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_grossly_out_of_range_pivot_is_flagged() {
        let record = rig_record(
            "head",
            [70, 92],
            [50_000.0, 4.0],
            [38.0, 42.0],
            Some("unknown"),
            None,
        );
        let diagnostics = check(&[record]);
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &diagnostics[0],
            Diagnostic::PivotOutOfBounds { id, .. } if id == "head"
        ));
    }

    #[test]
    fn a_non_positive_display_size_is_flagged() {
        let record = rig_record(
            "torso",
            [127, 173],
            [0.0, 6.0],
            [0.0, 74.0],
            Some("unknown"),
            None,
        );
        let diagnostics = check(&[record]);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::InvalidDisplaySize { id, .. } if id == "torso"))
        );
    }

    #[test]
    fn a_known_crop_rectangle_within_its_source_sheet_is_clean() {
        let mut records = vec![source_sheet_record("sheet", [512, 512])];
        records.push(rig_record(
            "head",
            [70, 92],
            [4.0, 60.0],
            [38.0, 42.0],
            Some("10,20,70,92"),
            Some("sheet"),
        ));
        let diagnostics = check(&records);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_known_crop_rectangle_exceeding_its_source_sheet_is_flagged() {
        let mut records = vec![source_sheet_record("sheet", [100, 100])];
        records.push(rig_record(
            "head",
            [70, 92],
            [4.0, 60.0],
            [38.0, 42.0],
            Some("50,50,70,92"),
            Some("sheet"),
        ));
        let diagnostics = check(&records);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::CropOutOfBounds { id, .. } if id == "head"))
        );
    }

    #[test]
    fn a_negative_crop_rectangle_is_flagged_even_without_a_resolvable_source_sheet() {
        let record = rig_record(
            "head",
            [70, 92],
            [4.0, 60.0],
            [38.0, 42.0],
            Some("-1,0,70,92"),
            None,
        );
        let diagnostics = check(&[record]);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::CropOutOfBounds { id, .. } if id == "head"))
        );
    }

    #[test]
    fn an_unknown_crop_sentinel_is_never_flagged() {
        let record = rig_record(
            "head",
            [70, 92],
            [4.0, 60.0],
            [38.0, 42.0],
            Some("unknown"),
            None,
        );
        assert!(check(&[record]).is_empty());
    }

    #[test]
    fn a_mild_aspect_distortion_within_tolerance_is_clean() {
        // Real data: fighters.strigoi.runtime.foot-front is ~2.27x.
        let record = rig_record(
            "foot-front",
            [66, 76],
            [18.86, -110.0],
            [21.28, 10.8],
            Some("unknown"),
            None,
        );
        assert!(check(&[record]).is_empty());
    }

    #[test]
    fn an_extreme_aspect_distortion_beyond_tolerance_is_flagged() {
        let record = rig_record(
            "distorted",
            [100, 100],
            [0.0, 0.0],
            [100.0, 5.0],
            Some("unknown"),
            None,
        );
        let diagnostics = check(&[record]);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::AspectDistortion { id, .. } if id == "distorted"))
        );
    }

    #[test]
    fn the_documented_known_aspect_distortion_failure_is_not_re_reported() {
        let record = rig_record(
            "foot-back",
            [41, 86],
            [-6.56, -110.0],
            [21.28, 10.8],
            Some("unknown"),
            None,
        );
        let mut record = record;
        record.record.id = "fighters.strigoi.runtime.foot-back".to_string();
        assert!(check(&[record]).is_empty());
    }
}
