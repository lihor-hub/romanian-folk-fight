//! Shared UI building blocks used by more than one screen: the button
//! bundles from the main-menu pattern and the attribute +/- allocation row
//! shared by character creation and the level-up panel.

pub mod attribute_row;

use bevy::prelude::*;

use crate::menu::{BUTTON_NORMAL, CREAM};

/// A small square button (name arrows, `-` / `+`).
pub fn small_button(label: &str) -> impl Bundle {
    button_bundle(label, Val::Px(48.0), Val::Px(48.0), 24.0)
}

/// A wide button (confirm, back, screen navigation), sized like the
/// main-menu buttons.
pub fn wide_button(label: &str) -> impl Bundle {
    button_bundle(label, Val::Px(260.0), Val::Px(56.0), 24.0)
}

/// A [`wide_button`] whose label text additionally carries `text_marker`, for
/// callers that update the label at runtime (e.g. the settings overlay's mute
/// toggle).
pub fn wide_button_labeled(label: &str, text_marker: impl Bundle) -> impl Bundle {
    labeled_button_bundle(label, Val::Px(260.0), Val::Px(56.0), 24.0, text_marker)
}

/// A button with a centered text label, mirroring the main-menu buttons.
pub fn button_bundle(label: &str, width: Val, height: Val, font_size: f32) -> impl Bundle {
    labeled_button_bundle(label, width, height, font_size, ())
}

/// The shared shape behind every button helper: the label text carries
/// `text_marker` (`()` when the caller never touches the label again).
pub fn labeled_button_bundle(
    label: &str,
    width: Val,
    height: Val,
    font_size: f32,
    text_marker: impl Bundle,
) -> impl Bundle {
    (
        Button,
        Node {
            width,
            height,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BUTTON_NORMAL),
        children![(
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(font_size),
                ..default()
            },
            TextColor(CREAM),
            text_marker,
        )],
    )
}
