//! Core plugin: global game states, camera, and screen-cleanup helpers.

use bevy::prelude::*;

/// Top-level flow of the game; every screen scopes its systems and entities
/// to one of these states.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    MainMenu,
    CharacterCreation,
    Shop,
    Fight,
    FightResult,
    GameOver,
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .add_systems(Startup, spawn_camera)
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu_placeholder)
            .add_systems(
                OnExit(GameState::MainMenu),
                despawn_screen::<MainMenuScreen>,
            );
    }
}

/// Despawns every entity tagged with the screen marker `T`. Register it in
/// `OnExit(...)` so a screen cleans up after itself.
pub fn despawn_screen<T: Component>(mut commands: Commands, entities: Query<Entity, With<T>>) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Marker for the placeholder main-menu screen; replaced by the real menu in
/// the main-menu feature issue.
#[derive(Component)]
struct MainMenuScreen;

fn spawn_main_menu_placeholder(mut commands: Commands) {
    commands.spawn((
        MainMenuScreen,
        Text::new("Romanian Folk Fight - Swords and Sandals"),
        TextFont {
            font_size: FontSize::Px(40.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(20.0),
            left: Val::Px(20.0),
            ..default()
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app
    }

    #[test]
    fn initial_state_is_main_menu() {
        let mut app = test_app();
        app.update();
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::MainMenu);
    }

    #[test]
    fn next_state_transition_applies_on_update() {
        let mut app = test_app();
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        let state = app.world().resource::<State<GameState>>();
        assert_eq!(*state.get(), GameState::Fight);
    }

    #[derive(Component)]
    struct TestScreen;

    #[test]
    fn despawn_screen_removes_only_tagged_entities() {
        let mut app = App::new();
        app.add_systems(Update, despawn_screen::<TestScreen>);
        let tagged = app.world_mut().spawn(TestScreen).id();
        let untagged = app.world_mut().spawn_empty().id();
        app.update();
        assert!(app.world().get_entity(tagged).is_err());
        assert!(app.world().get_entity(untagged).is_ok());
    }

    #[test]
    fn main_menu_placeholder_spawns_and_despawns_with_state() {
        let mut app = test_app();
        app.update();
        let count = |app: &mut App| {
            app.world_mut()
                .query_filtered::<(), With<MainMenuScreen>>()
                .iter(app.world())
                .count()
        };
        assert_eq!(count(&mut app), 1, "placeholder spawned on enter");
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update();
        assert_eq!(count(&mut app), 0, "placeholder despawned on exit");
    }
}
