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
/// space -- exact inverse of [`world_point_for_screen_point`]. Used by
/// `review::publish_palette_state`, which projects the staged fighter
/// positions (`arena::ArenaStaging`) through this to build its deterministic
/// fighter-readable-region proxy for the `fight-palette-phone` scenario's
/// obstruction check (#276). `creation` and `shop` used to reach this too
/// (#123, #273), for the letterboxed `WorldCamera` their preview rigs used
/// to render through; #247 moved those rigs to
/// [`super::PreviewCamera`](crate::core::PreviewCamera)'s own full-window
/// projection instead (see [`preview_world_point_for_screen_point`] and its
/// test-only inverse [`screen_point_for_preview_world_point`]), so this
/// letterbox-specific forward projection is now exclusively the `review`
/// seam's.
///
/// `#[cfg(feature = "review")]` rather than plain `pub(crate)` since nothing
/// in an ordinary `cargo build`/`trunk build --release`/`cargo test` needs
/// the letterboxed forward projection -- only the review seam does.
#[cfg(feature = "review")]
pub(crate) fn screen_point_for_world_point(world: Vec2, letterbox: LetterboxRect) -> Vec2 {
    let zoom = letterbox_zoom(letterbox);
    letterbox.position
        + Vec2::new(
            (world.x + LOGICAL_WIDTH / 2.0) * zoom,
            (LOGICAL_HEIGHT / 2.0 - world.y) * zoom,
        )
}

/// Inverse-projects a point in full-window logical screen space (top-left
/// origin, y-down) into the world-space point [`super::PreviewCamera`]
/// renders there (#247). Unlike [`world_point_for_screen_point`], this
/// camera is never letterboxed — one world unit is always exactly one
/// logical screen pixel over the *whole* window, so there is no
/// [`LetterboxRect`]/zoom factor to invert: `viewport` is simply the
/// window's current logical size (`ViewportInfo::width`/`height`). This is
/// the fix for #247: the letterboxed `WorldCamera` a creation/shop preview
/// rig used to be placed through (via [`world_point_for_screen_point`]) can
/// have a visible viewport far shorter than the preview frame's own on-screen
/// box on a narrow/tall (phone) window, permanently clipping part of the
/// frame to black no matter where the rig is positioned inside it.
pub(crate) fn preview_world_point_for_screen_point(screen: Vec2, viewport: Vec2) -> Vec2 {
    Vec2::new(screen.x - viewport.x / 2.0, viewport.y / 2.0 - screen.y)
}

/// The forward projection for [`preview_world_point_for_screen_point`] — see
/// [`screen_point_for_world_point`]'s doc comment for why this exists only
/// for `creation`'s and `shop`'s own tests to verify a preview rig's
/// resulting `Transform` actually lands back inside the `PreviewStage` rect
/// it was derived from.
#[cfg(test)]
pub(crate) fn screen_point_for_preview_world_point(world: Vec2, viewport: Vec2) -> Vec2 {
    Vec2::new(world.x + viewport.x / 2.0, viewport.y / 2.0 - world.y)
}
