//! The attribute +/- allocation cell: label above a decrease button, the
//! current value, and an increase button. Extracted from the creation screen
//! (#10) so the level-up panel on the result screen reuses the exact same
//! widget.
//!
//! #128 reshaped the row into a compact cell (label stacked over the
//! stepper) so eight attributes fit two-per-row inside the creation deck's
//! inner width and neither screen grows past the 900px-tall desktop
//! viewport; callers lay the cells out in a wrapping row.
//!
//! The cell is agnostic of who owns it: the caller passes the bundles to
//! attach to the `-` button, the `+` button, and the value label (its action
//! and label-marker components), and wires its own interaction systems.

use bevy::prelude::*;

use crate::character::AttributeKind;
use crate::core::UiFont;
use crate::theme::CREAM;

use super::small_button;

/// Fixed width of one attribute cell: `-` (48) + value slot (36) + `+` (48)
/// plus the two 6px gaps. Two cells and the 8px wrap gap fit the creation
/// deck's 308px inner width (see `creation`'s #216 width note): 2 * 144 + 8
/// = 296.
pub const ATTRIBUTE_CELL_WIDTH: f32 = 144.0;

/// Spawns one compact attribute cell under `parent`: the attribute's
/// Romanian label on top, then a `-` button carrying `decrease`, the current
/// `value` carrying `value_label`, and a `+` button carrying `increase`.
pub fn spawn_attribute_row(
    parent: &mut ChildSpawnerCommands,
    kind: AttributeKind,
    value: u32,
    decrease: impl Bundle,
    increase: impl Bundle,
    value_label: impl Bundle,
    ui_font: &UiFont,
) {
    parent
        .spawn(Node {
            width: Val::Px(ATTRIBUTE_CELL_WIDTH),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .with_children(|cell| {
            cell.spawn((
                Text::new(kind.label()),
                ui_font.text_font(15.0),
                TextColor(CREAM),
            ));
            cell.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((small_button("-", ui_font), decrease));
                row.spawn(Node {
                    width: Val::Px(36.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|slot| {
                    slot.spawn((
                        Text::new(value.to_string()),
                        ui_font.text_font(22.0),
                        TextColor(CREAM),
                        value_label,
                    ));
                });
                row.spawn((small_button("+", ui_font), increase));
            });
        });
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
