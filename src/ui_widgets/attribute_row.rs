//! The attribute +/- allocation row: label, decrease button, current value,
//! increase button. Extracted from the creation screen (#10) so the level-up
//! panel on the result screen reuses the exact same widget.
//!
//! The row is agnostic of who owns it: the caller passes the bundles to
//! attach to the `-` button, the `+` button, and the value label (its action
//! and label-marker components), and wires its own interaction systems.

use bevy::prelude::*;

use crate::character::AttributeKind;
use crate::core::UiFont;
use crate::menu::CREAM;

use super::small_button;

/// Spawns one attribute row under `parent`: the attribute's Romanian label,
/// a `-` button carrying `decrease`, the current `value` carrying
/// `value_label`, and a `+` button carrying `increase`.
pub fn spawn_attribute_row(
    parent: &mut ChildSpawnerCommands,
    kind: AttributeKind,
    value: u32,
    decrease: impl Bundle,
    increase: impl Bundle,
    value_label: impl Bundle,
    ui_font: &UiFont,
) {
    spawn_stepper_row(
        parent,
        kind.label(),
        value,
        decrease,
        increase,
        value_label,
        ui_font,
    );
}

/// The generic `-` / value / `+` stepper row behind [`spawn_attribute_row`],
/// reused by the settings overlay's volume steppers (#30): any label, a `-`
/// button carrying `decrease`, the current `value` carrying `value_label`,
/// and a `+` button carrying `increase`.
pub fn spawn_stepper_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: u32,
    decrease: impl Bundle,
    increase: impl Bundle,
    value_label: impl Bundle,
    ui_font: &UiFont,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                width: Val::Px(140.0),
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((Text::new(label), ui_font.text_font(24.0), TextColor(CREAM)));
            });
            row.spawn((small_button("-", ui_font), decrease));
            row.spawn(Node {
                width: Val::Px(48.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((
                    Text::new(value.to_string()),
                    ui_font.text_font(24.0),
                    TextColor(CREAM),
                    value_label,
                ));
            });
            row.spawn((small_button("+", ui_font), increase));
        });
}
