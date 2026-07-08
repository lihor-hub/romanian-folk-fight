//! Character creation screen: pick a folk hero name, distribute attribute
//! points, preview derived stats, and enter the arena.
//!
//! The allocation rules live in [`draft`] as pure logic; this module only
//! wires them to Bevy UI following the button pattern from the main menu.

pub mod draft;

use bevy::prelude::*;

pub use draft::{AttributeKind, CharacterDraft, FOLK_NAMES, FREE_POINTS};

use crate::character::{Attributes, stats};
use crate::core::{GameState, UiFont, despawn_screen};
use crate::menu::DisabledButton;
use crate::theme::{
    BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, NIGHT_BLACK,
    TEXT_DISABLED,
};
use crate::ui_widgets::{attribute_row::spawn_attribute_row, small_button, wide_button};

/// The confirmed player character: chosen name plus final attributes. Written
/// by the confirm button and read by the fight screen.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct PlayerCharacter {
    pub name: String,
    pub attributes: Attributes,
}

/// Marker for the creation-screen root; everything under it is despawned by
/// [`despawn_screen`] on `OnExit(GameState::CharacterCreation)`.
#[derive(Component)]
struct CreationScreen;

/// What a creation-screen button does when pressed (same approach as
/// `MenuAction` from the main menu).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum CreationAction {
    /// Cycle to the previous folk hero name.
    PreviousName,
    /// Cycle to the next folk hero name.
    NextName,
    /// Spend one free point on the attribute.
    Increase(AttributeKind),
    /// Refund one point from the attribute.
    Decrease(AttributeKind),
    /// Confirm the build, store [`PlayerCharacter`], and start the fight.
    Confirm,
    /// Return to the main menu and reset the draft.
    Back,
}

/// Which piece of draft state a text label displays; one generic system
/// refreshes all of them whenever the draft changes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum CreationLabel {
    Name,
    Points,
    Value(AttributeKind),
    Preview,
}

pub struct CreationPlugin;

impl Plugin for CreationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterDraft>()
            .add_plugins(crate::ui_widgets::ScrollInputPlugin)
            .add_systems(OnEnter(GameState::CharacterCreation), spawn_creation_screen)
            .add_systems(
                Update,
                (
                    handle_creation_actions,
                    update_button_backgrounds,
                    update_control_availability,
                    update_labels.run_if(resource_changed::<CharacterDraft>),
                    crate::ui_widgets::scroll_with_wheel_and_touch,
                )
                    .chain()
                    .run_if(in_state(GameState::CharacterCreation)),
            )
            .add_systems(
                OnExit(GameState::CharacterCreation),
                despawn_screen::<CreationScreen>,
            );
    }
}

fn spawn_creation_screen(mut commands: Commands, draft: Res<CharacterDraft>, ui_font: Res<UiFont>) {
    commands
        .spawn((
            CreationScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(12.0),
                // Attribute rows plus the preview can outgrow short
                // viewports (portrait phones, #31); scroll instead of
                // clipping unreachable controls.
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(NIGHT_BLACK),
            ScrollPosition::default(),
            crate::ui_widgets::Scrollable,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Creează-ți eroul"),
                ui_font.text_font_bold(44.0),
                TextColor(CREAM),
                Node {
                    margin: UiRect::bottom(Val::Px(20.0)),
                    ..default()
                },
            ));

            // Name selection: `<` and `>` cycle through the curated folk
            // hero names.
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(16.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((small_button("<", &ui_font), CreationAction::PreviousName));
                    row.spawn(Node {
                        width: Val::Px(320.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|slot| {
                        slot.spawn((
                            Text::new(draft.name()),
                            ui_font.text_font(30.0),
                            TextColor(CREAM),
                            CreationLabel::Name,
                        ));
                    });
                    row.spawn((small_button(">", &ui_font), CreationAction::NextName));
                });

            parent.spawn((
                Text::new(points_text(&draft)),
                ui_font.text_font(24.0),
                TextColor(CREAM),
                CreationLabel::Points,
                Node {
                    margin: UiRect::vertical(Val::Px(8.0)),
                    ..default()
                },
            ));

            for kind in AttributeKind::ALL {
                spawn_attribute_row(
                    parent,
                    kind,
                    draft.get(kind),
                    CreationAction::Decrease(kind),
                    CreationAction::Increase(kind),
                    CreationLabel::Value(kind),
                    &ui_font,
                );
            }

            parent.spawn((
                Text::new(preview_text(&draft)),
                ui_font.text_font(22.0),
                TextColor(CREAM),
                CreationLabel::Preview,
                Node {
                    margin: UiRect::vertical(Val::Px(8.0)),
                    ..default()
                },
            ));

            parent.spawn((
                wide_button("Începe lupta", &ui_font),
                CreationAction::Confirm,
            ));
            parent.spawn((wide_button("Înapoi", &ui_font), CreationAction::Back));
        });
}

/// The "points remaining" label text.
fn points_text(draft: &CharacterDraft) -> String {
    format!("Puncte rămase: {}", draft.points_remaining())
}

/// The derived-stat preview text, computed with the shared `stats` formulas.
fn preview_text(draft: &CharacterDraft) -> String {
    let attrs = draft.attributes();
    format!(
        "HP: {} | Stamina: {} | Damage: {}",
        stats::max_hp(&attrs),
        stats::max_stamina(&attrs),
        stats::base_damage(&attrs),
    )
}

/// Query filter: enabled buttons whose interaction changed this frame.
type ChangedEnabledButton = (Changed<Interaction>, With<Button>, Without<DisabledButton>);

/// Query data for [`update_control_availability`]: a button, its action,
/// whether it is currently disabled, and what it needs restyled.
type AvailabilityControlled = (
    Entity,
    &'static CreationAction,
    Has<DisabledButton>,
    &'static mut BackgroundColor,
    &'static Children,
);

/// Runs the [`CreationAction`] of whichever enabled button was pressed. The
/// draft methods enforce the allocation invariants, so a press that would
/// break them is simply a no-op.
fn handle_creation_actions(
    mut commands: Commands,
    interactions: Query<(&Interaction, &CreationAction), ChangedEnabledButton>,
    mut draft: ResMut<CharacterDraft>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            CreationAction::PreviousName => draft.previous_name(),
            CreationAction::NextName => draft.next_name(),
            CreationAction::Increase(kind) => {
                draft.increase(kind);
            }
            CreationAction::Decrease(kind) => {
                draft.decrease(kind);
            }
            CreationAction::Confirm => {
                if draft.is_complete() {
                    commands.insert_resource(PlayerCharacter {
                        name: draft.name().to_string(),
                        attributes: draft.attributes(),
                    });
                    // The build now lives in `PlayerCharacter`; reset so any
                    // later visit to the screen starts from a fresh draft.
                    draft.reset();
                    next_state.set(GameState::Fight);
                }
            }
            CreationAction::Back => {
                draft.reset();
                next_state.set(GameState::MainMenu);
            }
        }
    }
}

/// Hover/pressed background feedback for every enabled button (same pattern
/// as the main menu).
fn update_button_backgrounds(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedEnabledButton>,
) {
    for (interaction, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None => BUTTON_NORMAL,
        };
    }
}

/// Enables/disables `+`, `-`, and confirm buttons to match the draft: `+`
/// greys out with 0 points remaining, `-` at the base value, confirm until
/// all points are spent. Only touches buttons whose enabled state actually
/// flipped, so it does not fight the hover-feedback system.
fn update_control_availability(
    draft: Res<CharacterDraft>,
    mut commands: Commands,
    mut buttons: Query<AvailabilityControlled, With<Button>>,
    mut text_colors: Query<&mut TextColor>,
) {
    for (entity, action, was_disabled, mut background, children) in &mut buttons {
        let enabled = match action {
            CreationAction::Increase(_) => draft.can_increase(),
            CreationAction::Decrease(kind) => draft.can_decrease(*kind),
            CreationAction::Confirm => draft.is_complete(),
            _ => continue,
        };
        if enabled != was_disabled {
            // Already in the right state; leave hover feedback alone.
            continue;
        }
        let text_color = if enabled {
            commands.entity(entity).remove::<DisabledButton>();
            background.0 = BUTTON_NORMAL;
            CREAM
        } else {
            commands.entity(entity).insert(DisabledButton);
            background.0 = BUTTON_DISABLED;
            TEXT_DISABLED
        };
        for child in children.iter() {
            if let Ok(mut color) = text_colors.get_mut(child) {
                color.0 = text_color;
            }
        }
    }
}

/// Refreshes every [`CreationLabel`] text from the draft. Scheduled after the
/// action handler and gated on `resource_changed`, so the preview reacts on
/// the same frame as the click.
fn update_labels(draft: Res<CharacterDraft>, mut labels: Query<(&mut Text, &CreationLabel)>) {
    for (mut text, label) in &mut labels {
        text.0 = match label {
            CreationLabel::Name => draft.name().to_string(),
            CreationLabel::Points => points_text(&draft),
            CreationLabel::Value(kind) => draft.get(*kind).to_string(),
            CreationLabel::Preview => preview_text(&draft),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CorePlugin;
    use bevy::state::app::StatesPlugin;

    /// Headless app already sitting on the creation screen.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, CreationPlugin));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::CharacterCreation);
        app.update(); // transition + OnEnter spawn
        app.update(); // availability pass settles initial disabled states
        app
    }

    fn find_button(app: &mut App, action: CreationAction) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &CreationAction), With<Button>>()
            .iter(app.world())
            .find(|(_, a)| **a == action)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("button for {action:?} exists"))
    }

    fn press(app: &mut App, action: CreationAction) {
        let button = find_button(app, action);
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
    }

    fn draft(app: &App) -> &CharacterDraft {
        app.world().resource::<CharacterDraft>()
    }

    fn label_text(app: &mut App, wanted: CreationLabel) -> String {
        app.world_mut()
            .query::<(&Text, &CreationLabel)>()
            .iter(app.world())
            .find(|(_, label)| **label == wanted)
            .map(|(text, _)| text.0.clone())
            .unwrap_or_else(|| panic!("label {wanted:?} exists"))
    }

    #[test]
    fn entering_creation_spawns_the_screen() {
        let mut app = test_app();
        let roots = app
            .world_mut()
            .query_filtered::<(), With<CreationScreen>>()
            .iter(app.world())
            .count();
        assert_eq!(roots, 1, "creation screen root spawned");
        // 2 name arrows + 4 * (+/-) + confirm + back.
        let buttons = app
            .world_mut()
            .query_filtered::<(), With<Button>>()
            .iter(app.world())
            .count();
        assert_eq!(buttons, 12);
    }

    #[test]
    fn pressed_plus_mutates_the_draft_resource() {
        let mut app = test_app();
        press(&mut app, CreationAction::Increase(AttributeKind::Putere));
        assert_eq!(draft(&app).get(AttributeKind::Putere), 2);
        assert_eq!(draft(&app).points_remaining(), FREE_POINTS - 1);
    }

    #[test]
    fn minus_at_base_value_is_disabled_and_inert() {
        let mut app = test_app();
        let minus = find_button(&mut app, CreationAction::Decrease(AttributeKind::Noroc));
        assert!(
            app.world().entity(minus).contains::<DisabledButton>(),
            "`-` greys out at the base value"
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(minus).map(|b| b.0),
            Some(BUTTON_DISABLED)
        );
        press(&mut app, CreationAction::Decrease(AttributeKind::Noroc));
        assert_eq!(draft(&app).get(AttributeKind::Noroc), 1, "value unchanged");
        assert_eq!(draft(&app).points_remaining(), FREE_POINTS);
    }

    #[test]
    fn minus_enables_after_a_point_is_spent_and_refunds() {
        let mut app = test_app();
        press(&mut app, CreationAction::Increase(AttributeKind::Agilitate));
        let minus = find_button(&mut app, CreationAction::Decrease(AttributeKind::Agilitate));
        assert!(
            !app.world().entity(minus).contains::<DisabledButton>(),
            "`-` re-enables once above base"
        );
        press(&mut app, CreationAction::Decrease(AttributeKind::Agilitate));
        assert_eq!(draft(&app).get(AttributeKind::Agilitate), 1);
        assert_eq!(draft(&app).points_remaining(), FREE_POINTS);
    }

    #[test]
    fn plus_disables_at_zero_points_remaining() {
        let mut app = test_app();
        for _ in 0..FREE_POINTS {
            press(&mut app, CreationAction::Increase(AttributeKind::Putere));
        }
        assert_eq!(draft(&app).points_remaining(), 0);
        app.update();
        let plus = find_button(
            &mut app,
            CreationAction::Increase(AttributeKind::Vitalitate),
        );
        assert!(
            app.world().entity(plus).contains::<DisabledButton>(),
            "`+` greys out with no points left"
        );
        press(
            &mut app,
            CreationAction::Increase(AttributeKind::Vitalitate),
        );
        assert_eq!(
            draft(&app).get(AttributeKind::Vitalitate),
            1,
            "no overspend"
        );
    }

    #[test]
    fn confirm_is_gated_until_all_points_are_spent() {
        let mut app = test_app();
        let confirm = find_button(&mut app, CreationAction::Confirm);
        assert!(
            app.world().entity(confirm).contains::<DisabledButton>(),
            "confirm starts disabled"
        );
        press(&mut app, CreationAction::Confirm);
        app.update();
        assert!(
            app.world().get_resource::<PlayerCharacter>().is_none(),
            "no character before completion"
        );
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::CharacterCreation
        );
    }

    #[test]
    fn confirm_stores_player_character_and_starts_the_fight() {
        let mut app = test_app();
        press(&mut app, CreationAction::NextName);
        for _ in 0..4 {
            press(&mut app, CreationAction::Increase(AttributeKind::Putere));
        }
        for _ in 0..6 {
            press(
                &mut app,
                CreationAction::Increase(AttributeKind::Vitalitate),
            );
        }
        let confirm = find_button(&mut app, CreationAction::Confirm);
        assert!(
            !app.world().entity(confirm).contains::<DisabledButton>(),
            "confirm enables at exactly 10 spent"
        );
        press(&mut app, CreationAction::Confirm);
        app.update(); // transition applies

        let player = app
            .world()
            .get_resource::<PlayerCharacter>()
            .expect("PlayerCharacter stored on confirm");
        assert_eq!(player.name, FOLK_NAMES[1]);
        assert_eq!(
            player.attributes,
            Attributes {
                putere: 5,
                agilitate: 1,
                vitalitate: 7,
                noroc: 1,
            }
        );
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Fight
        );
        assert_eq!(
            *draft(&app),
            CharacterDraft::default(),
            "draft resets after confirm so a later visit starts fresh"
        );
        let leftovers = app
            .world_mut()
            .query_filtered::<(), With<CreationScreen>>()
            .iter(app.world())
            .count();
        assert_eq!(leftovers, 0, "screen despawned on exit");
    }

    #[test]
    fn back_returns_to_main_menu_and_resets_the_draft() {
        let mut app = test_app();
        press(&mut app, CreationAction::NextName);
        press(&mut app, CreationAction::Increase(AttributeKind::Noroc));
        press(&mut app, CreationAction::Back);
        app.update(); // transition applies

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu
        );
        assert_eq!(
            *draft(&app),
            CharacterDraft::default(),
            "re-entering shows a fresh draft"
        );
    }

    #[test]
    fn name_arrows_cycle_and_update_the_label() {
        let mut app = test_app();
        assert_eq!(label_text(&mut app, CreationLabel::Name), FOLK_NAMES[0]);
        press(&mut app, CreationAction::NextName);
        assert_eq!(draft(&app).name(), FOLK_NAMES[1]);
        assert_eq!(label_text(&mut app, CreationLabel::Name), FOLK_NAMES[1]);
        press(&mut app, CreationAction::PreviousName);
        press(&mut app, CreationAction::PreviousName);
        assert_eq!(
            label_text(&mut app, CreationLabel::Name),
            FOLK_NAMES[FOLK_NAMES.len() - 1],
            "wraps backwards"
        );
    }

    #[test]
    fn preview_and_points_labels_track_every_click() {
        let mut app = test_app();
        assert_eq!(
            label_text(&mut app, CreationLabel::Preview),
            "HP: 60 | Stamina: 35 | Damage: 3"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::Points),
            "Puncte rămase: 10"
        );
        press(
            &mut app,
            CreationAction::Increase(AttributeKind::Vitalitate),
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::Preview),
            "HP: 70 | Stamina: 40 | Damage: 3",
            "preview matches stats.rs formulas on the click frame"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::Points),
            "Puncte rămase: 9"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::Value(AttributeKind::Vitalitate)),
            "2"
        );
        press(&mut app, CreationAction::Increase(AttributeKind::Putere));
        assert_eq!(
            label_text(&mut app, CreationLabel::Preview),
            "HP: 70 | Stamina: 40 | Damage: 4"
        );
    }
}
