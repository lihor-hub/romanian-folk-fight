//! The victory ending screen (#26): shown when the lap-1 final boss falls.
//! The hero's name writ large, a victory announcer line, the run recap
//! (fights won, final level, lifetime galbeni earned), a short credits
//! block, and the two ways forward — **Turul 2** into the scaled ladder
//! loop or **Înapoi la menu** with the save kept.

use bevy::prelude::*;

use crate::announcer::{
    fill_placeholders,
    lines::{LineKey, pool},
};
use crate::core::GameState;
use crate::creation::PlayerCharacter;
use crate::menu::{CREAM, NIGHT_BLACK};
use crate::roster::{LADDER, LadderProgress};
use crate::ui_widgets::wide_button;

use super::{Level, LifetimeEarnings, result_ui::ChangedButton};

/// Muted tone for the credits block, so it reads as a footnote.
const CREDITS_GRAY: Color = Color::srgb(0.55, 0.52, 0.48);

/// Marker for the victory-screen root; despawned by
/// [`crate::core::despawn_screen`] on `OnExit(GameState::Victory)`.
#[derive(Component)]
pub struct VictoryScreen;

/// What a victory-screen button does when pressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VictoryAction {
    /// **Turul 2** → [`GameState::Shop`]: the run continues into the
    /// scaled ladder loop (the ladder already advanced past opponent 10).
    NextLap,
    /// **Înapoi la menu** → [`GameState::MainMenu`]. The run is *not*
    /// reset: the save is kept and **Continuă** resumes it.
    BackToMenu,
}

/// The victory announcer line for this win: a deterministic draw from the
/// [`LineKey::Victory`] pool (seeded by the run's earnings so different runs
/// vary), with the hero and the fallen boss filled in.
pub(super) fn victory_line(hero: &str, seed: u32) -> String {
    let lines = pool(LineKey::Victory);
    let boss = LADDER[LADDER.len() - 1].name;
    fill_placeholders(lines[seed as usize % lines.len()], hero, boss, 0)
}

/// Spawns the victory screen. Runs after `award_victory` credited the final
/// payout and advanced the ladder, so the recap shows the finished run:
/// fights won is the ladder position, the level and earnings include the
/// last fight's award.
pub(super) fn spawn_victory_screen(
    mut commands: Commands,
    player: Option<Res<PlayerCharacter>>,
    level: Res<Level>,
    earnings: Res<LifetimeEarnings>,
    ladder: Option<Res<LadderProgress>>,
) {
    let hero = player.map_or_else(|| "Voinicul".to_string(), |player| player.name.clone());
    let fights_won = ladder.map_or(LADDER.len(), |ladder| ladder.0);
    let line = victory_line(&hero, earnings.0);
    commands
        .spawn((screen_root(), VictoryScreen))
        .with_children(|parent| {
            parent.spawn(hero_name(&hero));
            parent.spawn(screen_title("Ai învins!"));
            parent.spawn(screen_line(
                "Legenda ta se va cânta la șezători.".to_string(),
            ));
            parent.spawn(screen_line(line));
            parent.spawn(screen_line(format!("Lupte câștigate: {fights_won}")));
            parent.spawn(screen_line(format!("Nivel atins: {}", level.level)));
            parent.spawn(screen_line(format!("Galbeni câștigați: {}", earnings.0)));
            parent.spawn((wide_button("Turul 2"), VictoryAction::NextLap));
            parent.spawn((wide_button("Înapoi la menu"), VictoryAction::BackToMenu));
            spawn_credits(parent);
        });
}

/// The short credits block: game name, tech, and the asset-credits pointer.
fn spawn_credits(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(4.0),
            margin: UiRect::top(Val::Px(24.0)),
            ..default()
        })
        .with_children(|credits| {
            for text in [
                "Romanian Folk Fight",
                "Făurit în Rust cu motorul Bevy",
                "Grafică: placeholder-e proprii (CC0) — vezi assets/CREDITS.md",
            ] {
                credits.spawn(credits_line(text));
            }
        });
}

/// Full-screen centered column, same layout as the other end screens.
fn screen_root() -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(NIGHT_BLACK),
    )
}

/// The hero's name, writ large above the title.
fn hero_name(name: &str) -> impl Bundle {
    (
        Text::new(name),
        TextFont {
            font_size: FontSize::Px(72.0),
            ..default()
        },
        TextColor(CREAM),
    )
}

/// The victory headline.
fn screen_title(label: &str) -> impl Bundle {
    (
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(48.0),
            ..default()
        },
        TextColor(CREAM),
        Node {
            margin: UiRect::bottom(Val::Px(16.0)),
            ..default()
        },
    )
}

/// One line of the recap text.
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

/// One muted line of the credits block.
fn credits_line(label: &str) -> impl Bundle {
    (
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(CREDITS_GRAY),
    )
}

/// Runs the [`VictoryAction`] of whichever victory-screen button was
/// pressed. Neither path resets the run: **Turul 2** heads to the shop with
/// the ladder already on lap 2, **Înapoi la menu** leaves the save intact so
/// **Continuă** resumes the looping run.
pub(super) fn handle_victory_actions(
    interactions: Query<(&Interaction, &VictoryAction), ChangedButton>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        next_state.set(match action {
            VictoryAction::NextLap => GameState::Shop,
            VictoryAction::BackToMenu => GameState::MainMenu,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FightOutcome, ProgressionPlugin, Wallet, fight_reward};
    use super::*;
    use crate::character::Attributes;
    use crate::combat::{CombatLogEvent, CombatSide};
    use crate::core::CorePlugin;
    use bevy::state::app::StatesPlugin;

    /// Headless app one update away from the victory screen: the run stands
    /// on the last lap-1 fight with the boss's outcome recorded, and the
    /// player has a name and some prior earnings.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, ProgressionPlugin));
        app.add_message::<CombatLogEvent>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes::default(),
        });
        app.insert_resource(LadderProgress(9));
        app.insert_resource(LifetimeEarnings(500));
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 10, true));
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

    /// Presses the victory-screen button carrying `action`.
    fn press_victory_button(app: &mut App, action: VictoryAction) {
        let button = app
            .world_mut()
            .query_filtered::<(Entity, &VictoryAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(entity, _)| entity)
            .expect("victory button exists");
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
    }

    #[test]
    fn the_victory_screen_shows_the_hero_the_recap_and_the_credits() {
        let mut app = test_app();
        set_state(&mut app, GameState::Victory);

        let texts = texts(&mut app);
        assert!(texts.contains(&"Făt-Frumos".to_string()), "{texts:?}");
        assert!(texts.contains(&"Ai învins!".to_string()), "{texts:?}");
        assert!(
            texts.contains(&"Legenda ta se va cânta la șezători.".to_string()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"Lupte câștigate: 10".to_string()),
            "the ladder advanced to 10 before the recap: {texts:?}"
        );
        assert!(
            texts.contains(&"Nivel atins: 3".to_string()),
            "the boss's 400 XP lifts level 1 to 3: {texts:?}"
        );
        assert!(
            texts.contains(&format!("Galbeni câștigați: {}", 500 + fight_reward(10))),
            "lifetime earnings include the final payout: {texts:?}"
        );
        assert!(
            texts.contains(&"Romanian Folk Fight".to_string())
                && texts.contains(&"Făurit în Rust cu motorul Bevy".to_string()),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.contains("assets/CREDITS.md")),
            "the credits point at the asset list: {texts:?}"
        );
        assert!(
            texts.contains(&"Turul 2".to_string()) && texts.contains(&"Înapoi la menu".to_string()),
            "{texts:?}"
        );
    }

    #[test]
    fn the_victory_line_comes_filled_from_the_victory_pool() {
        let hero = "Ileana Cosânzeana";
        for seed in 0..7 {
            let line = victory_line(hero, seed);
            assert!(line.contains(hero), "{line}");
            assert!(!line.contains('{'), "unfilled placeholder: {line}");
            assert!(
                pool(LineKey::Victory)
                    .iter()
                    .any(|template| fill_placeholders(template, hero, "Zmeul Zmeilor", 0) == line),
                "{line} comes from the pool"
            );
        }
    }

    #[test]
    fn turul_2_continues_the_run_into_the_scaled_ladder_via_the_shop() {
        let mut app = test_app();
        set_state(&mut app, GameState::Victory);

        press_victory_button(&mut app, VictoryAction::NextLap);

        assert_eq!(state(&app), GameState::Shop);
        let ladder = *app.world().resource::<LadderProgress>();
        assert_eq!(ladder, LadderProgress(10), "the run sits on lap 2");
        assert_eq!(ladder.lap(), 2);
        assert!(
            app.world().get_resource::<PlayerCharacter>().is_some(),
            "the hero carries on"
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<VictoryScreen>>()
                .iter(app.world())
                .count(),
            0,
            "screen despawned"
        );
    }

    #[test]
    fn back_to_menu_keeps_the_run_intact() {
        let mut app = test_app();
        set_state(&mut app, GameState::Victory);
        let wallet = *app.world().resource::<Wallet>();
        let earnings = *app.world().resource::<LifetimeEarnings>();

        press_victory_button(&mut app, VictoryAction::BackToMenu);

        assert_eq!(state(&app), GameState::MainMenu);
        assert_eq!(
            *app.world().resource::<Wallet>(),
            wallet,
            "no run reset on the way out"
        );
        assert_eq!(*app.world().resource::<LifetimeEarnings>(), earnings);
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress(10),
            "the looping run survives for Continuă"
        );
        assert!(
            app.world().get_resource::<PlayerCharacter>().is_some(),
            "the character is kept with the save"
        );
    }
}
