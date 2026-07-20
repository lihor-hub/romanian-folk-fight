//! Shared letterbox/world-camera projection helpers (#282).
//!
//! [`super::letterbox_camera`] keeps a fixed [`LOGICAL_WIDTH`]x[`LOGICAL_HEIGHT`]
//! world area on screen inside [`LetterboxRect`], adding black bars rather
//! than stretching or cropping. Any screen that needs to place a
//! world-space object (a preview rig) at a UI node's on-screen position, or
//! read a UI node's resolved layout rect in that same screen-pixel space,
//! goes through the helpers below.
//!
//! Consolidated here by #282: `creation` (#123/#274), `shop` (#273/#286),
//! and `review` each grew an identical (or near-identical) copy of this
//! math, plus `ui_widgets::focus::scroll_focused_into_view` had its own
//! inline variant of [`logical_node_rect`]. With four call sites the
//! trade-off flipped from "each screen documents the duplication as
//! deliberate precedent" to "a future letterbox/camera change must be
//! applied in several places, and missing one silently misplaces a preview
//! rig or a focus scroll" -- so this module is now the one canonical copy.

use bevy::prelude::*;

use super::{LOGICAL_HEIGHT, LOGICAL_WIDTH, LetterboxRect};

/// A UI node's resolved on-screen rect, in the same logical-pixel space
/// (top-left origin, y-down) [`LetterboxRect`] is expressed in --
/// `ComputedNode::size` is in physical pixels and `UiGlobalTransform`'s
/// translation places the node's center in physical-pixel space (matching
/// `ComputedNode::contains_point`'s own convention), so both are scaled back
/// to logical pixels by the node's own `inverse_scale_factor`.
pub(crate) fn logical_node_rect(transform: &UiGlobalTransform, node: &ComputedNode) -> Rect {
    let scale = node.inverse_scale_factor();
    Rect::from_center_size(transform.translation * scale, node.size() * scale)
}

/// How many logical screen pixels one world unit currently occupies:
/// [`LetterboxRect::size`] is the on-screen rect (in the same logical pixels
/// [`ComputedNode`] resolves to) the letterboxed world camera's `Fixed`
/// projection stretches its fixed [`LOGICAL_WIDTH`] x [`LOGICAL_HEIGHT`]
/// world area across, so this ratio is exactly 1.0 at the design resolution
/// (no bars), bigger on a wide desktop window (more screen room for the same
/// fixed world area), smaller on a narrow phone width. Falls back to `1.0` on
/// a not-yet-computed (zero-size) rect rather than dividing by zero.
///
/// Named `preview_zoom` in `creation`/`shop` before #282's consolidation;
/// renamed here since it is no longer specific to a "preview" screen.
pub(crate) fn letterbox_zoom(letterbox: LetterboxRect) -> f32 {
    if letterbox.size.x <= 0.0 {
        1.0
    } else {
        letterbox.size.x / LOGICAL_WIDTH
    }
}

/// Inverse-projects a point in full-window logical screen space (top-left
/// origin, y-down -- [`ComputedNode`]/[`LetterboxRect`]'s shared convention)
/// into the world-space point the letterboxed world camera renders there.
/// This is the fix for #123 (creation) and #273 (shop): the old
/// `*_preview_x_for_width` helpers derived a preview rig's position from
/// `ViewportInfo::width` alone, implicitly assuming world space and UI
/// screen space were the same 1:1 coordinate system -- only true when the
/// window happened to be exactly [`LOGICAL_WIDTH`] x [`LOGICAL_HEIGHT`] (no
/// letterbox bars). This derives it from a UI node's *actual* resolved
/// screen rect instead (see [`logical_node_rect`]).
pub(crate) fn world_point_for_screen_point(screen: Vec2, letterbox: LetterboxRect) -> Vec2 {
    let zoom = letterbox_zoom(letterbox);
    let local = screen - letterbox.position;
    Vec2::new(
        local.x / zoom - LOGICAL_WIDTH / 2.0,
        LOGICAL_HEIGHT / 2.0 - local.y / zoom,
    )
}

/// The forward projection -- world space back to full-window logical screen
/// space -- exact inverse of [`world_point_for_screen_point`]. A plain
/// (non-`review`) production build never needs this (a preview rig is only
/// ever placed screen -> world); `creation`'s and `shop`'s tests use it to
/// verify a rig's resulting `Transform` actually lands back inside the
/// `PreviewStage` rect it was derived from (#123, #273). #276 adds a second,
/// real (feature-gated) production caller: `review::publish_palette_state`
/// projects the staged fighter positions (`arena::ArenaStaging`) through
/// this to build its deterministic fighter-readable-region proxy for the
/// `fight-palette-phone` scenario's obstruction check.
///
/// `#[cfg(any(test, feature = "review"))]` rather than plain `pub(crate)`
/// since nothing in an ordinary `cargo build`/`trunk build --release` needs
/// the forward projection -- only tests and the review seam do. Still
/// reachable from `creation`'s and `shop`'s own `#[cfg(test)] mod tests`:
/// `cfg(test)` applies to the whole crate for a given `cargo test`
/// compilation, not per-module, so this item exists in the same build as
/// every other module's test code.
#[cfg(any(test, feature = "review"))]
pub(crate) fn screen_point_for_world_point(world: Vec2, letterbox: LetterboxRect) -> Vec2 {
    let zoom = letterbox_zoom(letterbox);
    letterbox.position
        + Vec2::new(
            (world.x + LOGICAL_WIDTH / 2.0) * zoom,
            (LOGICAL_HEIGHT / 2.0 - world.y) * zoom,
        )
}
