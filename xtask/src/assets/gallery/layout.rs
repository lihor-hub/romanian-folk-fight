//! Rig-space -> CSS-pixel layout math for composition pages (#197).
//!
//! # Coordinate convention
//!
//! `pivot`/`display` (read straight from the sidecar aggregate, never
//! duplicated as separate numbers here) are in the same rig-space unit
//! system documented in `xtask/src/assets/validate/bounds.rs`: `pivot` is
//! the rig-space translation of a part's own sprite center from the rig
//! root, matching `src/cutout.rs`'s `Transform::from_xyz(offset.x,
//! offset.y, z)` / `CutoutPart::offset`; `display` is that sprite's runtime
//! width/height, matching `Sprite::custom_size`. `+x` is toward the
//! character's authored-facing (right, pre-mirror) side; `+y` is up (screen
//! space, not rig-space, grows down -- this module flips the sign when
//! converting).
//!
//! A composition assembles every part by *translating* it to `pivot` and
//! *sizing* it to `display`; it does **not** reproduce the small per-part
//! rotations `src/cutout.rs` additionally applies at rest pose (rotation is
//! not tracked in any sidecar field -- see `xtask/README.md`'s "Known
//! limitations" and this issue's instruction not to duplicate template
//! metadata). The result is an unrotated approximation of the rest pose:
//! correct proportions, placement, and gear attachment, not a pixel-exact
//! game screenshot. Every composition page states this explicitly.
//!
//! Mirroring (`flip_x`) matches both runtime operations in `src/cutout.rs`:
//! `part_transform` mirrors the transform/pivot, while `part_sprite` flips
//! the sprite's own pixels. Composition pages reproduce both effects.
//! Standalone pages isolate the same pixel flip as a review aid.

/// One part's placement input: rig-space pivot (translation) and display
/// size (both already `f32` straight from the sidecar aggregate).
#[derive(Debug, Clone, Copy)]
pub struct PartPlacement {
    pub pivot: [f32; 2],
    pub display: [f32; 2],
}

/// A resolved CSS box (all values already in `px`, top-left origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box2D {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// The shared canvas every part in one composition is placed against.
#[derive(Debug, Clone, Copy)]
pub struct RigCanvas {
    pub width: f32,
    pub height: f32,
    center_x: f32,
    center_y: f32,
}

/// Margin (px) added around the tightest bounding box of every part, so
/// silhouette edges are never clipped by the canvas border.
const MARGIN: f32 = 16.0;

/// Computes the shared canvas for a set of parts. The horizontal extent is
/// made symmetric (`+/-extent_x`) so the same canvas works unmodified for
/// both the normal and mirrored facing (placement negates each part's
/// `pivot.x`; the caller also mirrors each sprite's pixels).
pub fn rig_canvas(parts: &[PartPlacement]) -> RigCanvas {
    let mut extent_x: f32 = 1.0;
    let mut min_y: f32 = 0.0;
    let mut max_y: f32 = 1.0;
    for (i, part) in parts.iter().enumerate() {
        let half_w = part.display[0].abs() / 2.0;
        let half_h = part.display[1].abs() / 2.0;
        extent_x = extent_x.max(part.pivot[0].abs() + half_w);
        let top = part.pivot[1] + half_h;
        let bottom = part.pivot[1] - half_h;
        if i == 0 {
            min_y = bottom;
            max_y = top;
        } else {
            min_y = min_y.min(bottom);
            max_y = max_y.max(top);
        }
    }
    let width = 2.0 * extent_x + 2.0 * MARGIN;
    let height = (max_y - min_y) + 2.0 * MARGIN;
    RigCanvas {
        width,
        height,
        center_x: extent_x + MARGIN,
        center_y: max_y + MARGIN,
    }
}

impl RigCanvas {
    /// The screen-space `(x, y)` position of the rig-space origin `(0, 0)`
    /// on this canvas -- where a pivot/attachment-guide crosshair belongs.
    pub fn origin(&self) -> (f32, f32) {
        (self.center_x, self.center_y)
    }

    /// The CSS box for one part placed on this canvas. `mirrored` negates
    /// the rig-space `x` component, matching the translation portion of
    /// `src/cutout.rs`'s `part_transform`; the caller mirrors the pixels.
    pub fn place(&self, part: PartPlacement, mirrored: bool) -> Box2D {
        let rig_x = if mirrored {
            -part.pivot[0]
        } else {
            part.pivot[0]
        };
        let rig_y = part.pivot[1];
        let screen_x = self.center_x + rig_x;
        let screen_y = self.center_y - rig_y;
        Box2D {
            left: screen_x - part.display[0] / 2.0,
            top: screen_y - part.display[1] / 2.0,
            width: part.display[0],
            height: part.display[1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(pivot: [f32; 2], display: [f32; 2]) -> PartPlacement {
        PartPlacement { pivot, display }
    }

    #[test]
    fn a_single_centered_part_sits_in_the_middle_of_its_canvas() {
        let parts = vec![part([0.0, 0.0], [40.0, 40.0])];
        let canvas = rig_canvas(&parts);
        let box2d = canvas.place(parts[0], false);
        // Canvas center_x = extent_x(20) + MARGIN; box left = center - 20.
        assert_eq!(box2d.width, 40.0);
        assert_eq!(box2d.height, 40.0);
        assert!((box2d.left - MARGIN).abs() < 0.01);
        assert!((box2d.top - MARGIN).abs() < 0.01);
    }

    #[test]
    fn mirroring_negates_only_the_x_component() {
        let parts = vec![
            part([-20.0, 10.0], [10.0, 10.0]),
            part([20.0, 10.0], [10.0, 10.0]),
        ];
        let canvas = rig_canvas(&parts);
        let normal = canvas.place(parts[0], false);
        let mirrored = canvas.place(parts[0], true);
        // Mirroring part[0] (pivot.x = -20) should land where part[1]
        // (pivot.x = +20) sits unmirrored, since the canvas is symmetric.
        let sibling_normal = canvas.place(parts[1], false);
        assert!((mirrored.left - sibling_normal.left).abs() < 0.01);
        assert_eq!(normal.top, mirrored.top, "y is never affected by mirroring");
    }

    #[test]
    fn canvas_extent_grows_to_fit_the_widest_and_tallest_parts() {
        let parts = vec![
            part([0.0, 0.0], [10.0, 10.0]),
            part([50.0, 100.0], [20.0, 20.0]),
        ];
        let canvas = rig_canvas(&parts);
        // extent_x = max(0+5, 50+10) = 60 -> width = 120 + 2*MARGIN.
        assert!((canvas.width - (120.0 + 2.0 * MARGIN)).abs() < 0.01);
        // max_y = 100+10=110, min_y = min(0-5, 100-10)= -5 -> height = 115 + 2*MARGIN
        assert!((canvas.height - (115.0 + 2.0 * MARGIN)).abs() < 0.01);
    }
}
