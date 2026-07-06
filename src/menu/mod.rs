//! Main menu plugin: title screen, menu buttons, and the reusable
//! [`MenuAction`] button-interaction system that later screens copy.

use bevy::prelude::*;

use crate::core::{GameState, despawn_screen};

// Placeholder folk palette (deep red / cream / black); real art comes in
// Phase 4. Public so later screens (e.g. character creation) share the exact
// same styling until a dedicated ui module exists.
pub const DEEP_RED: Color = Color::srgb(0.55, 0.10, 0.10);
pub const CREAM: Color = Color::srgb(0.96, 0.93, 0.84);
pub const NIGHT_BLACK: Color = Color::srgb(0.07, 0.06, 0.06);

pub const BUTTON_NORMAL: Color = DEEP_RED;
pub const BUTTON_HOVERED: Color = Color::srgb(0.68, 0.16, 0.14);
pub const BUTTON_PRESSED: Color = Color::srgb(0.42, 0.06, 0.06);
pub const BUTTON_DISABLED: Color = Color::srgb(0.35, 0.33, 0.31);
pub const TEXT_DISABLED: Color = Color::srgb(0.60, 0.58, 0.55);

/// Marker for the main-menu screen root; everything under it is despawned by
/// [`despawn_screen`] on `OnExit(GameState::MainMenu)`.
#[derive(Component)]
struct MainMenuScreen;

/// What a menu button does when pressed. Attach it next to [`Button`] and the
/// generic [`handle_menu_actions`] system takes care of the rest; no
/// per-button system needed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Start a new game (transition to [`GameState::CharacterCreation`]).
    NewGame,
    /// Quit the app; native builds only.
    #[cfg(not(target_arch = "wasm32"))]
    Quit,
}

/// Marker for buttons that are greyed out and ignore all interaction.
#[derive(Component)]
pub struct DisabledButton;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(
                Update,
                (handle_menu_actions, update_button_backgrounds)
                    .run_if(in_state(GameState::MainMenu)),
            )
            .add_systems(
                OnExit(GameState::MainMenu),
                despawn_screen::<MainMenuScreen>,
            );
    }
}

fn spawn_main_menu(mut commands: Commands) {
    commands
        .spawn((
            MainMenuScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(NIGHT_BLACK),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Romanian Folk Fight"),
                TextFont {
                    font_size: FontSize::Px(56.0),
                    ..default()
                },
                TextColor(CREAM),
                Node {
                    margin: UiRect::bottom(Val::Px(32.0)),
                    ..default()
                },
            ));
            parent.spawn((
                menu_button("Luptă nouă", CREAM, BUTTON_NORMAL),
                MenuAction::NewGame,
            ));
            // Disabled until the save/load issue wires it up.
            parent.spawn((
                menu_button("Continuă", TEXT_DISABLED, BUTTON_DISABLED),
                DisabledButton,
            ));
            #[cfg(not(target_arch = "wasm32"))]
            parent.spawn((menu_button("Ieși", CREAM, BUTTON_NORMAL), MenuAction::Quit));
        });
}

/// A menu button with a centered text label.
fn menu_button(label: &str, text_color: Color, background: Color) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(260.0),
            height: Val::Px(56.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(background),
        children![(
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(24.0),
                ..default()
            },
            TextColor(text_color),
        )],
    )
}

/// Query filter: buttons whose interaction changed this frame.
type ChangedButton = (Changed<Interaction>, With<Button>);

/// Query filter: like [`ChangedButton`], but skipping disabled buttons.
type ChangedEnabledButton = (Changed<Interaction>, With<Button>, Without<DisabledButton>);

/// Generic click handler: runs the [`MenuAction`] of whichever button was
/// pressed. Disabled buttons never carry a `MenuAction`, so they are ignored.
fn handle_menu_actions(
    interactions: Query<(&Interaction, &MenuAction), ChangedButton>,
    mut next_state: ResMut<NextState<GameState>>,
    #[cfg(not(target_arch = "wasm32"))] mut app_exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            MenuAction::NewGame => next_state.set(GameState::CharacterCreation),
            #[cfg(not(target_arch = "wasm32"))]
            MenuAction::Quit => {
                app_exit.write(AppExit::Success);
            }
        }
    }
}

/// Hover/pressed background feedback for every enabled button.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CorePlugin;
    use bevy::state::app::StatesPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, MenuPlugin));
        app
    }

    fn count<C: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<C>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn pressing_new_game_queues_character_creation() {
        let mut app = test_app();
        app.update();
        app.world_mut()
            .spawn((Button, Interaction::Pressed, MenuAction::NewGame));
        app.update();
        let next = app.world().resource::<NextState<GameState>>();
        assert!(
            matches!(*next, NextState::Pending(GameState::CharacterCreation)),
            "pressing Luptă nouă must queue CharacterCreation"
        );
    }

    #[test]
    fn menu_spawns_on_enter_and_despawns_fully_on_new_game() {
        let mut app = test_app();
        app.update();
        assert_eq!(count::<MainMenuScreen>(&mut app), 1, "menu root spawned");
        let expected_buttons = if cfg!(target_arch = "wasm32") { 2 } else { 3 };
        assert_eq!(count::<Button>(&mut app), expected_buttons);

        let new_game = app
            .world_mut()
            .query_filtered::<Entity, With<MenuAction>>()
            .iter(app.world())
            .find(|&e| app.world().get::<MenuAction>(e) == Some(&MenuAction::NewGame))
            .expect("New Game button exists");
        app.world_mut()
            .entity_mut(new_game)
            .insert(Interaction::Pressed);
        app.update(); // handler queues the transition
        app.update(); // transition applies, OnExit runs

        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::CharacterCreation);
        assert_eq!(count::<MainMenuScreen>(&mut app), 0, "root despawned");
        assert_eq!(count::<Button>(&mut app), 0, "buttons despawned");
        assert_eq!(count::<Text>(&mut app), 0, "labels and title despawned");
    }

    #[test]
    fn disabled_continue_button_ignores_presses() {
        let mut app = test_app();
        app.update();
        let continue_button = app
            .world_mut()
            .query_filtered::<Entity, With<DisabledButton>>()
            .iter(app.world())
            .next()
            .expect("Continuă button exists");
        app.world_mut()
            .entity_mut(continue_button)
            .insert(Interaction::Pressed);
        app.update();
        app.update();

        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::MainMenu, "state unchanged");
        let background = app.world().get::<BackgroundColor>(continue_button);
        assert_eq!(
            background.map(|b| b.0),
            Some(BUTTON_DISABLED),
            "disabled button keeps its greyed-out background"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pressing_quit_writes_app_exit() {
        let mut app = test_app();
        app.update();
        app.world_mut()
            .spawn((Button, Interaction::Pressed, MenuAction::Quit));
        app.update();
        assert!(app.should_exit().is_some(), "quit must raise AppExit");
    }

    #[test]
    fn enabled_button_background_tracks_interaction() {
        let mut app = test_app();
        app.update();
        let button = app
            .world_mut()
            .spawn((Button, Interaction::Hovered, BackgroundColor(BUTTON_NORMAL)))
            .id();
        app.update();
        let bg = |app: &App| app.world().get::<BackgroundColor>(button).unwrap().0;
        assert_eq!(bg(&app), BUTTON_HOVERED, "hover feedback");
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        assert_eq!(bg(&app), BUTTON_PRESSED, "pressed feedback");
        app.world_mut().entity_mut(button).insert(Interaction::None);
        app.update();
        assert_eq!(bg(&app), BUTTON_NORMAL, "returns to normal");
    }
}
