//! Shared UI building blocks used by more than one screen: the button
//! bundles from the main-menu pattern, the attribute +/- allocation row
//! shared by character creation and the level-up panel, and the
//! wheel/touch-drag scroll behavior (#31) shared by the shop and creation
//! screens.

pub mod attribute_row;
pub mod focus;

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::core::UiFont;
use crate::theme::{BUTTON_NORMAL, CREAM};
use focus::{Focusable, TabIndex};

/// Registers the input types [`scroll_with_wheel_and_touch`] reads
/// (`MouseWheel` messages, the `Touches` resource) so screens that use
/// [`Scrollable`] work in headless test apps that skip the full
/// `InputPlugin`/`WindowPlugin` stack. Idempotent — safe to add from more
/// than one screen plugin.
pub struct ScrollInputPlugin;

impl Plugin for ScrollInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MouseWheel>().init_resource::<Touches>();
    }

    fn is_unique(&self) -> bool {
        false
    }
}

/// Marker for a scrollable `Node` (must also carry `Overflow::scroll_y()`
/// and a `ScrollPosition`). Lets the shop and creation screens (#31) scroll
/// their content into view on short viewports via mouse wheel, trackpad, or
/// a touch drag — Bevy UI clips and offsets children from `ScrollPosition`
/// automatically; this system just drives that value from input.
#[derive(Component)]
pub struct Scrollable;

/// Applies mouse-wheel and single-finger touch-drag deltas to every
/// [`Scrollable`] node's [`ScrollPosition`]. Touch deltas are inverted (drag
/// up to scroll down, matching native touch-scroll conventions); wheel
/// deltas are used as-is. Clamping to content bounds is handled by Bevy UI's
/// layout system, which snaps an out-of-range `ScrollPosition` back in range.
pub fn scroll_with_wheel_and_touch(
    mut wheel: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    mut scrollables: Query<&mut ScrollPosition, With<Scrollable>>,
) {
    let mut delta_y = 0.0;
    for event in wheel.read() {
        delta_y += event.y;
    }
    for touch in touches.iter() {
        delta_y -= touch.delta().y;
    }
    if delta_y == 0.0 {
        return;
    }
    for mut scroll in &mut scrollables {
        scroll.0.y -= delta_y;
        scroll.0.y = scroll.0.y.max(0.0);
    }
}

/// A small square button (name arrows, `-` / `+`).
pub fn small_button(label: &str, ui_font: &UiFont) -> impl Bundle {
    button_bundle(label, Val::Px(48.0), Val::Px(48.0), 24.0, ui_font)
}

/// A wide button (confirm, back, screen navigation), sized like the
/// main-menu buttons.
pub fn wide_button(label: &str, ui_font: &UiFont) -> impl Bundle {
    button_bundle(label, Val::Px(260.0), Val::Px(56.0), 24.0, ui_font)
}

/// A [`wide_button`] whose label text additionally carries `text_marker`, for
/// callers that update the label at runtime (e.g. the settings overlay's mute
/// toggle).
pub fn wide_button_labeled(label: &str, text_marker: impl Bundle, ui_font: &UiFont) -> impl Bundle {
    labeled_button_bundle(
        label,
        Val::Px(260.0),
        Val::Px(56.0),
        24.0,
        text_marker,
        ui_font,
    )
}

/// A button with a centered text label, mirroring the main-menu buttons.
pub fn button_bundle(
    label: &str,
    width: Val,
    height: Val,
    font_size: f32,
    ui_font: &UiFont,
) -> impl Bundle {
    labeled_button_bundle(label, width, height, font_size, (), ui_font)
}

/// The shared shape behind every button helper: the label text carries
/// `text_marker` (`()` when the caller never touches the label again).
///
/// Every button built through this helper is [`Focusable`] with `TabIndex(0)`
/// (#216): the caller only has to wrap the screen's focusable root in a
/// [`focus::TabGroup`] (see that module's registration API) for the button to
/// join keyboard/gamepad tab order automatically -- including a button the
/// caller marks `crate::menu::DisabledButton` afterwards, which per that
/// module's own contract must *stay* focusable so its disabled reason is
/// still reachable, just inert on activation. This is why every current and
/// future screen using [`button_bundle`], [`wide_button`],
/// [`wide_button_labeled`], or [`small_button`] gets #216's rollout "for
/// free" and never needs its own bespoke focus wiring.
pub fn labeled_button_bundle(
    label: &str,
    width: Val,
    height: Val,
    font_size: f32,
    text_marker: impl Bundle,
    ui_font: &UiFont,
) -> impl Bundle {
    (
        Button,
        Focusable,
        TabIndex(0),
        Node {
            width,
            height,
            // #216: a button is an interactive target with a hard 44px
            // floor (`theme::MIN_TOUCH_TARGET`) -- never let a tight flex
            // row squeeze it below its designed size (the row's text slots,
            // which have no minimum, absorb the deficit instead). The
            // `touch-targets` browser gate measures the *rendered* box, so
            // a squeezed button fails the run even when its authored width
            // was compliant.
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BUTTON_NORMAL),
        children![(
            Text::new(label),
            ui_font.text_font(font_size),
            TextColor(CREAM),
            text_marker,
        )],
    )
}
