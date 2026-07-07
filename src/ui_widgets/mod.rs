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

/// A button with a centered text label, mirroring the main-menu buttons.
pub fn button_bundle(label: &str, width: Val, height: Val, font_size: f32) -> impl Bundle {
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
        )],
    )
}
