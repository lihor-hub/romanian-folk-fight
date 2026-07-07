//! The two end-of-fight screens: the victory result screen (payout breakdown
//! plus the shop / next-fight choice) and the game-over screen (run reset back
//! to the main menu). Both follow the button pattern from the main menu.

use bevy::prelude::*;

use crate::core::GameState;
use crate::menu::{BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, NIGHT_BLACK};

use super::{FightOutcome, Wallet, reset_run};

/// Marker for the victory-result screen root; despawned by
/// [`crate::core::despawn_screen`] on `OnExit(GameState::FightResult)`.
#[derive(Component)]
pub struct ResultScreen;

/// Marker for the game-over screen root; despawned on
/// `OnExit(GameState::GameOver)`.
#[derive(Component)]
pub struct GameOverScreen;

/// What a result-screen button does when pressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultAction {
    /// Spend the payout (**La prăvălie** → [`GameState::Shop`]).
    GoToShop,
    /// Straight into the next duel (**Lupta următoare** →
    /// [`GameState::Fight`]; the arena and combat respawn via their own
    /// `OnEnter` systems).
    NextFight,
}

/// What a game-over-screen button does when pressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOverAction {
    /// **Înapoi la menu** → [`GameState::MainMenu`], resetting the run.
    BackToMenu,
}

/// Spawns the victory screen: title, payout breakdown, wallet total, and the
/// shop / next-fight buttons. Runs after the wallet was credited, so the
/// shown total already includes the reward.
pub(super) fn spawn_result_screen(
    mut commands: Commands,
    outcome: Option<Res<FightOutcome>>,
    wallet: Res<Wallet>,
) {
    let (reward, xp) = match outcome {
        Some(outcome) => (outcome.reward, outcome.xp),
        None => {
            warn!("entered GameState::FightResult without a FightOutcome; showing zeros");
            (0, 0)
        }
    };
    commands
        .spawn((screen_root(), ResultScreen))
        .with_children(|parent| {
            parent.spawn(screen_title("Victorie!"));
            parent.spawn(screen_line(format!("Recompensă: {reward} galbeni")));
            parent.spawn(screen_line(format!("Experiență: {xp} XP")));
            parent.spawn(screen_line(format!("Pungă: {} galbeni", wallet.0)));
            parent.spawn((screen_button("La prăvălie"), ResultAction::GoToShop));
            parent.spawn((screen_button("Lupta următoare"), ResultAction::NextFight));
        });
}

/// Spawns the game-over screen: epitaph, the run's galbeni total, and the
/// back-to-menu button.
pub(super) fn spawn_game_over_screen(mut commands: Commands, wallet: Res<Wallet>) {
    commands
        .spawn((screen_root(), GameOverScreen))
        .with_children(|parent| {
            parent.spawn(screen_title("Ai fost răpus…"));
            parent.spawn(screen_line(format!("Galbeni strânși: {}", wallet.0)));
            parent.spawn((screen_button("Înapoi la menu"), GameOverAction::BackToMenu));
        });
}

/// Full-screen centered column, same layout as the main menu.
fn screen_root() -> impl Bundle {
    (
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
    )
}

/// Large screen title with the same styling as the main-menu title.
fn screen_title(label: &str) -> impl Bundle {
    (
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(56.0),
            ..default()
        },
        TextColor(CREAM),
        Node {
            margin: UiRect::bottom(Val::Px(32.0)),
            ..default()
        },
    )
}

/// One line of the breakdown text.
fn screen_line(label: String) -> impl Bundle {
    (
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(CREAM),
    )
}

/// A button with a centered text label, mirroring the main-menu buttons.
fn screen_button(label: &str) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(260.0),
            height: Val::Px(56.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BUTTON_NORMAL),
        children![(
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(24.0),
                ..default()
            },
            TextColor(CREAM),
        )],
    )
}

/// Query filter: buttons whose interaction changed this frame.
type ChangedButton = (Changed<Interaction>, With<Button>);

/// Runs the [`ResultAction`] of whichever result-screen button was pressed.
pub(super) fn handle_result_actions(
    interactions: Query<(&Interaction, &ResultAction), ChangedButton>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        next_state.set(match action {
            ResultAction::GoToShop => GameState::Shop,
            ResultAction::NextFight => GameState::Fight,
        });
    }
}

/// Runs the [`GameOverAction`] of whichever game-over button was pressed:
/// back to the menu with every run resource reset.
pub(super) fn handle_game_over_actions(
    mut commands: Commands,
    interactions: Query<(&Interaction, &GameOverAction), ChangedButton>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            GameOverAction::BackToMenu => {
                reset_run(&mut commands);
                next_state.set(GameState::MainMenu);
            }
        }
    }
}

/// Hover/pressed background feedback, same palette as the main menu.
pub(super) fn update_button_backgrounds(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedButton>,
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
    use super::super::{FIGHT_END_DELAY_SECONDS, FightEndDelay, ProgressionPlugin};
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::character::{Attributes, EnemyFighter, Health, PlayerFighter, Stamina};
    use crate::combat::{CombatEvent, CombatLogEvent, CombatPlugin, CombatSide};
    use crate::core::CorePlugin;
    use crate::creation::PlayerCharacter;
    use bevy::state::app::StatesPlugin;
    use std::time::Duration;

    /// Headless app with only the progression flow (no arena or combat).
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ProgressionPlugin));
        app.add_message::<CombatLogEvent>();
        app.update();
        app
    }

    /// Same player build as the arena/combat tests: agilitate 2 ties the
    /// Strigoi, so the player opens and combat idles without input.
    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    /// Headless app with the full fight loop: arena, combat, progression.
    fn full_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin, ProgressionPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
        });
        app.update();
        app
    }

    fn set_state(app: &mut App, state: GameState) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(state);
        app.update();
    }

    fn state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    fn texts(app: &mut App) -> Vec<String> {
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect()
    }

    fn count<C: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<C>>()
            .iter(app.world())
            .count()
    }

    /// Presses `button`: the handler queues the transition on the first
    /// update, the second update applies it and runs OnExit/OnEnter.
    fn press(app: &mut App, button: Entity) {
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
    }

    /// Presses the result-screen button carrying `action`.
    fn press_result_button(app: &mut App, action: ResultAction) {
        let button = app
            .world_mut()
            .query_filtered::<(Entity, &ResultAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(entity, _)| entity)
            .expect("result button exists");
        press(app, button);
    }

    /// Presses the game-over screen's back-to-menu button.
    fn press_back_to_menu(app: &mut App) {
        let button = app
            .world_mut()
            .query_filtered::<Entity, (With<Button>, With<GameOverAction>)>()
            .single(app.world())
            .expect("back-to-menu button exists");
        press(app, button);
    }

    #[test]
    fn the_victory_screen_shows_the_payout_and_the_credited_wallet() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1));
        set_state(&mut app, GameState::FightResult);

        let texts = texts(&mut app);
        assert!(texts.contains(&"Victorie!".to_string()), "{texts:?}");
        assert!(
            texts.contains(&"Recompensă: 35 galbeni".to_string()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"Pungă: 85 galbeni".to_string()),
            "the shown total already includes the reward: {texts:?}"
        );
        assert!(
            texts.contains(&"La prăvălie".to_string())
                && texts.contains(&"Lupta următoare".to_string()),
            "{texts:?}"
        );
    }

    #[test]
    fn la_pravalie_leads_to_the_shop_and_the_screen_despawns() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1));
        set_state(&mut app, GameState::FightResult);

        press_result_button(&mut app, ResultAction::GoToShop);

        assert_eq!(state(&app), GameState::Shop);
        assert_eq!(count::<ResultScreen>(&mut app), 0, "root despawned");
        assert_eq!(count::<Button>(&mut app), 0, "buttons despawned");
        assert_eq!(count::<Text>(&mut app), 0, "labels despawned");
    }

    #[test]
    fn the_game_over_screen_shows_the_run_total() {
        let mut app = test_app();
        app.insert_resource(Wallet(123));
        set_state(&mut app, GameState::GameOver);

        let texts = texts(&mut app);
        assert!(texts.contains(&"Ai fost răpus…".to_string()), "{texts:?}");
        assert!(
            texts.contains(&"Galbeni strânși: 123".to_string()),
            "{texts:?}"
        );
    }

    #[test]
    fn back_to_menu_resets_the_run() {
        let mut app = test_app();
        app.insert_resource(Wallet(123));
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes::default(),
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Enemy, 1));
        set_state(&mut app, GameState::GameOver);

        press_back_to_menu(&mut app);

        assert_eq!(state(&app), GameState::MainMenu);
        assert_eq!(
            *app.world().resource::<Wallet>(),
            Wallet::default(),
            "wallet back to the starting galbeni"
        );
        assert!(
            app.world().get_resource::<PlayerCharacter>().is_none(),
            "character cleared for a fresh run"
        );
        assert!(
            app.world().get_resource::<FightOutcome>().is_none(),
            "no stale outcome survives the run"
        );
        assert_eq!(count::<GameOverScreen>(&mut app), 0, "screen despawned");
        assert_eq!(count::<Button>(&mut app), 0, "buttons despawned");
    }

    #[test]
    fn lupta_urmatoare_starts_a_fresh_fight_at_full_pools() {
        let mut app = full_app();
        set_state(&mut app, GameState::Fight);

        // Wound the player, then land the killing blow on the enemy.
        app.world_mut()
            .query_filtered::<&mut Health, With<PlayerFighter>>()
            .single_mut(app.world_mut())
            .expect("player fighter exists")
            .current = 1;
        app.world_mut().write_message(CombatLogEvent {
            actor: CombatSide::Player,
            event: CombatEvent::Defeated,
        });
        app.update();
        app.world_mut()
            .resource_mut::<FightEndDelay>()
            .0
            .tick(Duration::from_secs_f32(FIGHT_END_DELAY_SECONDS + 1.0));
        app.update();
        app.update();
        assert_eq!(state(&app), GameState::FightResult);

        press_result_button(&mut app, ResultAction::NextFight);

        assert_eq!(state(&app), GameState::Fight);
        assert!(
            app.world().get_resource::<FightOutcome>().is_none(),
            "the fresh fight starts with no outcome"
        );
        let (player_health, player_stamina) = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<PlayerFighter>>()
            .single(app.world())
            .expect("exactly one player fighter respawned");
        assert_eq!(
            player_health.current, player_health.max,
            "the wounded player is back at full HP"
        );
        assert_eq!(player_stamina.current, player_stamina.max);
        let (enemy_health, enemy_stamina) = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<EnemyFighter>>()
            .single(app.world())
            .expect("exactly one enemy fighter respawned");
        assert_eq!(enemy_health.current, enemy_health.max);
        assert_eq!(enemy_stamina.current, enemy_stamina.max);
    }
}
