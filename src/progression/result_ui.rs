//! The two end-of-fight screens: the victory result screen (payout breakdown,
//! XP progress, the level-up point allocation, and the shop / next-fight
//! choice) and the game-over screen (run reset back to the main menu). Both
//! follow the button pattern from the main menu.

use bevy::prelude::*;

use crate::character::{AttributeKind, Health, PlayerFighter, Stamina, stats};
use crate::combat::hud::bar_percent;
use crate::core::UiFont;
use crate::creation::PlayerCharacter;
use crate::flow::FlowIntent;
use crate::save::SaveRequested;
use crate::theme::{
    BAR_TRACK, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, NIGHT_BLACK, PanelTexture,
    STAMINA_FILL, panel_bundle,
};
use crate::ui_widgets::{attribute_row::spawn_attribute_row, wide_button};

use super::{FightOutcome, Level, LevelUpDraft, Wallet, reset_run, top_up_pool, xp_to_next};

/// Width of the XP progress bar.
const XP_BAR_WIDTH: f32 = 300.0;
/// Height of the XP progress bar.
const XP_BAR_HEIGHT: f32 = 12.0;

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

/// Marker for the level-up point-allocation panel on the result screen;
/// despawned when the allocation is confirmed (or with the whole screen).
#[derive(Component)]
pub struct AllocationPanel;

/// What a level-up allocation button does when pressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocateAction {
    /// Spend one unspent point on the attribute.
    Increase(AttributeKind),
    /// Refund one point allocated in this session.
    Decrease(AttributeKind),
    /// Apply the allocation to [`PlayerCharacter`] (**Confirmă**); leftover
    /// points stay on [`Level`] for later.
    Confirm,
}

/// Which piece of the allocation draft a text label displays.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationLabel {
    /// The "points remaining" line.
    Points,
    /// One attribute's current value.
    Value(AttributeKind),
}

/// Spawns the victory screen: title, payout breakdown, wallet total, XP
/// progress towards the next level, the point-allocation panel when there
/// are unspent points, and the shop / next-fight buttons. Runs after the
/// wallet and XP were credited, so the shown totals already include the
/// award.
pub(super) fn spawn_result_screen(
    mut commands: Commands,
    outcome: Option<Res<FightOutcome>>,
    wallet: Res<Wallet>,
    level: Res<Level>,
    player: Option<Res<PlayerCharacter>>,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
) {
    let (reward, xp) = match outcome {
        Some(outcome) => (outcome.reward, outcome.xp),
        None => {
            warn!("entered GameState::FightResult without a FightOutcome; showing zeros");
            (0, 0)
        }
    };
    // A fresh allocation draft over the confirmed build; unspent points from
    // earlier fights are offered again until they are finally spent.
    let draft = player
        .filter(|_| level.unspent_points > 0)
        .map(|player| LevelUpDraft::new(player.attributes, level.unspent_points));
    commands
        .spawn((screen_root(), ResultScreen))
        .with_children(|screen| {
            screen
                .spawn(panel_bundle(
                    &panel_texture,
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(16.0),
                        padding: UiRect::all(Val::Px(28.0)),
                        ..default()
                    },
                ))
                .with_children(|parent| {
                    parent.spawn(screen_title("Victorie!", &ui_font));
                    parent.spawn(screen_line(
                        format!("Recompensă: {reward} galbeni"),
                        &ui_font,
                    ));
                    parent.spawn(screen_line(format!("Experiență: {xp} XP"), &ui_font));
                    parent.spawn(screen_line(
                        format!("Pungă: {} galbeni", wallet.0),
                        &ui_font,
                    ));
                    parent.spawn(screen_line(
                        format!(
                            "Nivel {} — XP: {}/{}",
                            level.level,
                            level.xp,
                            xp_to_next(level.level)
                        ),
                        &ui_font,
                    ));
                    parent.spawn(xp_bar(&level));
                    if let Some(draft) = &draft {
                        spawn_allocation_panel(parent, draft, &ui_font);
                    }
                    parent.spawn((wide_button("La prăvălie", &ui_font), ResultAction::GoToShop));
                    parent.spawn((
                        wide_button("Lupta următoare", &ui_font),
                        ResultAction::NextFight,
                    ));
                });
        });
    if let Some(draft) = draft {
        commands.insert_resource(draft);
    }
}

/// The XP progress bar: a carved-wood track with a thin gold edge and a fill
/// sized to the progress towards the next level (same visual language as the
/// HUD pool bars).
fn xp_bar(level: &Level) -> impl Bundle {
    let percent = bar_percent(level.xp as i32, xp_to_next(level.level) as i32);
    (
        Node {
            width: Val::Px(XP_BAR_WIDTH),
            height: Val::Px(XP_BAR_HEIGHT),
            border: UiRect::all(Val::Px(1.5)),
            ..default()
        },
        BackgroundColor(BAR_TRACK),
        BorderColor::all(crate::theme::GOLD),
        children![(
            Node {
                width: Val::Percent(percent),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(STAMINA_FILL),
        )],
    )
}

/// The "points remaining" label text of the allocation panel.
fn points_text(draft: &LevelUpDraft) -> String {
    format!("Puncte de atribut: {}", draft.points_remaining())
}

/// Spawns the level-up allocation panel: the points-remaining line, one
/// shared attribute row per attribute (the same widget as the creation
/// screen), and the confirm button.
fn spawn_allocation_panel(
    parent: &mut ChildSpawnerCommands,
    draft: &LevelUpDraft,
    ui_font: &UiFont,
) {
    parent
        .spawn((
            AllocationPanel,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                margin: UiRect::vertical(Val::Px(8.0)),
                ..default()
            },
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(points_text(draft)),
                ui_font.text_font(24.0),
                TextColor(CREAM),
                AllocationLabel::Points,
            ));
            for kind in AttributeKind::ALL {
                spawn_attribute_row(
                    panel,
                    kind,
                    draft.get(kind),
                    AllocateAction::Decrease(kind),
                    AllocateAction::Increase(kind),
                    AllocationLabel::Value(kind),
                    ui_font,
                );
            }
            panel.spawn((wide_button("Confirmă", ui_font), AllocateAction::Confirm));
        });
}

/// Spawns the game-over screen: epitaph, the run's galbeni total, and the
/// back-to-menu button.
pub(super) fn spawn_game_over_screen(
    mut commands: Commands,
    wallet: Res<Wallet>,
    ui_font: Res<UiFont>,
) {
    commands
        .spawn((screen_root(), GameOverScreen))
        .with_children(|parent| {
            parent.spawn(screen_title("Ai fost răpus…", &ui_font));
            parent.spawn(screen_line(
                format!("Galbeni strânși: {}", wallet.0),
                &ui_font,
            ));
            parent.spawn((
                wide_button("Înapoi la menu", &ui_font),
                GameOverAction::BackToMenu,
            ));
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
fn screen_title(label: &str, ui_font: &UiFont) -> impl Bundle {
    (
        Text::new(label),
        ui_font.text_font_bold(56.0),
        TextColor(CREAM),
        Node {
            margin: UiRect::bottom(Val::Px(32.0)),
            ..default()
        },
    )
}

/// One line of the breakdown text.
fn screen_line(label: String, ui_font: &UiFont) -> impl Bundle {
    (Text::new(label), ui_font.text_font(24.0), TextColor(CREAM))
}

/// Query filter: buttons whose interaction changed this frame.
pub(super) type ChangedButton = (Changed<Interaction>, With<Button>);

/// Runs the [`ResultAction`] of whichever result-screen button was pressed:
/// emits the matching [`FlowIntent`] — the payout was already credited on
/// `OnEnter(FightResult)`, so this handler has no domain side effect of its
/// own to order before the intent.
pub(super) fn handle_result_actions(
    interactions: Query<(&Interaction, &ResultAction), ChangedButton>,
    mut intents: MessageWriter<FlowIntent>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        intents.write(match action {
            ResultAction::GoToShop => FlowIntent::GoToShop,
            ResultAction::NextFight => FlowIntent::NextFight,
        });
    }
}

/// Runs the [`AllocateAction`] of whichever allocation button was pressed.
/// The draft methods enforce the invariants (no overspend, never below the
/// confirmed build), so a press that would break them is a no-op. Confirm
/// applies the draft: [`PlayerCharacter`] takes the new attributes, a live
/// player fighter's pools top up by exactly the vitalitate max-delta,
/// leftover points persist on [`Level`], the panel closes, and the new
/// build is autosaved (see [`crate::save`]).
// A Bevy system: each parameter is a distinct ECS handle the confirm branch
// needs (draft, level, player, fighter pools, panel, autosave trigger).
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_allocation_actions(
    mut commands: Commands,
    interactions: Query<(&Interaction, &AllocateAction), ChangedButton>,
    draft: Option<ResMut<LevelUpDraft>>,
    level: Option<ResMut<Level>>,
    player: Option<ResMut<PlayerCharacter>>,
    mut fighters: Query<(&mut Health, &mut Stamina), With<PlayerFighter>>,
    panels: Query<Entity, With<AllocationPanel>>,
    mut save_requests: MessageWriter<SaveRequested>,
) {
    let (Some(mut draft), Some(mut level), Some(mut player)) = (draft, level, player) else {
        return;
    };
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            AllocateAction::Increase(kind) => {
                draft.increase(kind);
            }
            AllocateAction::Decrease(kind) => {
                draft.decrease(kind);
            }
            AllocateAction::Confirm => {
                let attributes = draft.attributes();
                for (mut health, mut stamina) in &mut fighters {
                    (health.current, health.max) =
                        top_up_pool(health.current, health.max, stats::max_hp(&attributes));
                    (stamina.current, stamina.max) = top_up_pool(
                        stamina.current,
                        stamina.max,
                        stats::max_stamina(&attributes),
                    );
                }
                player.attributes = attributes;
                level.unspent_points = draft.points_remaining();
                commands.remove_resource::<LevelUpDraft>();
                for panel in &panels {
                    commands.entity(panel).despawn();
                }
                save_requests.write(SaveRequested);
            }
        }
    }
}

/// Refreshes every [`AllocationLabel`] text from the draft. Scheduled after
/// the action handler and gated on `resource_exists_and_changed`, so labels
/// react on the same frame as the click.
pub(super) fn update_allocation_labels(
    draft: Res<LevelUpDraft>,
    mut labels: Query<(&mut Text, &AllocationLabel)>,
) {
    for (mut text, label) in &mut labels {
        text.0 = match label {
            AllocationLabel::Points => points_text(&draft),
            AllocationLabel::Value(kind) => draft.get(*kind).to_string(),
        };
    }
}

/// Runs the [`GameOverAction`] of whichever game-over button was pressed:
/// resets every run resource, then emits [`FlowIntent::BackToMenu`] — the
/// reset must land before the intent so the flow table never routes to the
/// menu with stale run state.
pub(super) fn handle_game_over_actions(
    mut commands: Commands,
    interactions: Query<(&Interaction, &GameOverAction), ChangedButton>,
    mut intents: MessageWriter<FlowIntent>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            GameOverAction::BackToMenu => {
                reset_run(&mut commands);
                intents.write(FlowIntent::BackToMenu);
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
    use crate::core::{CorePlugin, GameState};
    use crate::creation::PlayerCharacter;
    use crate::flow::FlowPlugin;
    use bevy::state::app::StatesPlugin;
    use std::time::Duration;

    /// Headless app with only the progression flow (no arena or combat).
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            ProgressionPlugin,
        ));
        app.add_message::<CombatLogEvent>();
        app.update();
        app
    }

    /// Same player build as the arena/combat tests: agilitate 2 ties the
    /// Hoț de codru, so the player opens and combat idles without input.
    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    /// Headless app with the full fight loop: arena, combat, progression.
    fn full_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, FlowPlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin, ProgressionPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
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

    /// Presses the allocation-panel button carrying `action`.
    fn press_allocate_button(app: &mut App, action: AllocateAction) {
        let button = app
            .world_mut()
            .query_filtered::<(Entity, &AllocateAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(entity, _)| entity)
            .expect("allocation button exists");
        press(app, button);
    }

    /// A confirmed default-build character for allocation tests.
    fn default_character() -> PlayerCharacter {
        PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: Attributes::default(),
            appearance: crate::character::PlayerAppearance::default(),
        }
    }

    #[test]
    fn the_victory_screen_shows_the_payout_and_the_credited_wallet() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
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
    fn the_victory_screen_shows_the_xp_progress_after_the_award() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);

        let texts = texts(&mut app);
        assert!(
            texts.contains(&"Nivel 1 — XP: 20/100".to_string()),
            "the shown progress already includes the award: {texts:?}"
        );
        assert_eq!(
            count::<AllocationPanel>(&mut app),
            0,
            "no unspent points, no allocation panel"
        );
        assert!(
            app.world().get_resource::<LevelUpDraft>().is_none(),
            "no draft without points to spend"
        );
    }

    #[test]
    fn unspent_points_open_the_allocation_panel() {
        let mut app = test_app();
        app.insert_resource(default_character());
        app.insert_resource(Level {
            level: 2,
            xp: 0,
            unspent_points: 2,
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);

        assert_eq!(count::<AllocationPanel>(&mut app), 1, "panel spawned");
        let texts = texts(&mut app);
        assert!(
            texts.contains(&"Puncte de atribut: 2".to_string()),
            "{texts:?}"
        );
        for kind in AttributeKind::ALL {
            assert!(texts.contains(&kind.label().to_string()), "{texts:?}");
        }
        assert!(texts.contains(&"Confirmă".to_string()), "{texts:?}");
        let draft = app
            .world()
            .get_resource::<LevelUpDraft>()
            .expect("a fresh draft over the confirmed build");
        assert_eq!(draft.attributes(), Attributes::default());
        assert_eq!(draft.points_remaining(), 2);
    }

    #[test]
    fn allocation_clicks_drive_the_draft_and_its_labels() {
        let mut app = test_app();
        app.insert_resource(default_character());
        app.insert_resource(Level {
            level: 2,
            xp: 0,
            unspent_points: 2,
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);

        press_allocate_button(
            &mut app,
            AllocateAction::Increase(AttributeKind::Vitalitate),
        );
        let draft = app.world().resource::<LevelUpDraft>();
        assert_eq!(draft.get(AttributeKind::Vitalitate), 2);
        assert_eq!(draft.points_remaining(), 1);
        let texts = texts(&mut app);
        assert!(
            texts.contains(&"Puncte de atribut: 1".to_string()),
            "{texts:?}"
        );

        press_allocate_button(
            &mut app,
            AllocateAction::Decrease(AttributeKind::Vitalitate),
        );
        let draft = app.world().resource::<LevelUpDraft>();
        assert_eq!(draft.get(AttributeKind::Vitalitate), 1, "refunded");
        assert_eq!(draft.points_remaining(), 2);

        press_allocate_button(
            &mut app,
            AllocateAction::Decrease(AttributeKind::Vitalitate),
        );
        assert_eq!(
            app.world().resource::<LevelUpDraft>().attributes(),
            Attributes::default(),
            "the confirmed build is the floor"
        );
    }

    #[test]
    fn confirm_applies_the_allocation_and_keeps_leftover_points() {
        let mut app = test_app();
        app.insert_resource(default_character());
        app.insert_resource(Level {
            level: 2,
            xp: 10,
            unspent_points: 3,
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);
        // A wounded player fighter left over from the fight: 41/60 HP,
        // 10/35 stamina (the default build's pools).
        app.world_mut().spawn((
            PlayerFighter,
            Health {
                current: 41,
                max: 60,
            },
            Stamina {
                current: 10,
                max: 35,
            },
        ));

        press_allocate_button(
            &mut app,
            AllocateAction::Increase(AttributeKind::Vitalitate),
        );
        press_allocate_button(
            &mut app,
            AllocateAction::Increase(AttributeKind::Vitalitate),
        );
        press_allocate_button(&mut app, AllocateAction::Confirm);

        let player = app.world().resource::<PlayerCharacter>();
        assert_eq!(
            player.attributes,
            Attributes {
                vitalitate: 3,
                ..Attributes::default()
            },
            "the confirmed build gains the allocation"
        );
        assert_eq!(
            app.world().resource::<Level>().unspent_points,
            1,
            "the unspent point persists"
        );
        assert!(
            app.world().get_resource::<LevelUpDraft>().is_none(),
            "the draft is consumed"
        );
        assert_eq!(count::<AllocationPanel>(&mut app), 0, "panel closed");

        let (health, stamina) = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<PlayerFighter>>()
            .single(app.world())
            .expect("the fighter survives the confirm");
        assert_eq!(
            *health,
            Health {
                current: 61,
                max: 80,
            },
            "current HP tops up by exactly the max-delta (60 → 80)"
        );
        assert_eq!(
            *stamina,
            Stamina {
                current: 20,
                max: 45,
            },
            "current stamina tops up by exactly the max-delta (35 → 45)"
        );
    }

    #[test]
    fn leaving_without_confirm_keeps_the_points_for_the_next_visit() {
        let mut app = test_app();
        app.insert_resource(default_character());
        app.insert_resource(Level {
            level: 2,
            xp: 0,
            unspent_points: 2,
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);

        press_allocate_button(&mut app, AllocateAction::Increase(AttributeKind::Putere));
        press_result_button(&mut app, ResultAction::GoToShop);

        assert_eq!(state(&app), GameState::Shop);
        assert!(
            app.world().get_resource::<LevelUpDraft>().is_none(),
            "the abandoned draft is dropped"
        );
        assert_eq!(
            app.world().resource::<PlayerCharacter>().attributes,
            Attributes::default(),
            "nothing applies without confirm"
        );
        assert_eq!(
            app.world().resource::<Level>().unspent_points,
            2,
            "unconfirmed points stay unspent"
        );

        set_state(&mut app, GameState::FightResult);
        let draft = app
            .world()
            .get_resource::<LevelUpDraft>()
            .expect("a fresh draft on re-entry");
        assert_eq!(draft.points_remaining(), 2, "all points offered again");
        assert_eq!(draft.attributes(), Attributes::default());
    }

    #[test]
    fn la_pravalie_leads_to_the_shop_and_the_screen_despawns() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
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
        app.insert_resource(crate::roster::LadderProgress(7));
        app.insert_resource(default_character());
        app.insert_resource(Level {
            level: 4,
            xp: 55,
            unspent_points: 3,
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Enemy, 1, false));
        set_state(&mut app, GameState::GameOver);

        press_back_to_menu(&mut app);

        assert_eq!(state(&app), GameState::MainMenu);
        assert_eq!(
            *app.world().resource::<Wallet>(),
            Wallet::default(),
            "wallet back to the starting galbeni"
        );
        assert_eq!(
            *app.world().resource::<Level>(),
            Level::default(),
            "level back to 1 with nothing gathered"
        );
        assert_eq!(
            *app.world().resource::<crate::roster::LadderProgress>(),
            crate::roster::LadderProgress::default(),
            "the ladder starts over with the run"
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
            action: crate::combat::CombatAction::QuickStrike,
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
