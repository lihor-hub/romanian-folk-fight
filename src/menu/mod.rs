//! Main menu plugin: title screen, menu buttons, and the reusable
//! [`MenuAction`] button-interaction system that later screens copy.

use bevy::prelude::*;

use crate::core::{GameState, UiFont, despawn_screen};
use crate::flow::FlowIntent;
use crate::save::{SaveStore, load_save};
use crate::settings::SettingsOpen;
use crate::theme::{
    ARENA_BROWN, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, GOLD,
    PANEL_LINEN, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};

const MENU_ROOT_PADDING: f32 = 18.0;
const MENU_TITLE_STAGE_WIDTH: f32 = 382.0;
const MENU_BUTTON_PANEL_WIDTH: f32 = 318.0;

/// Marker for the main-menu screen root; everything under it is despawned by
/// [`despawn_screen`] on `OnExit(GameState::MainMenu)`.
#[derive(Component)]
struct MainMenuScreen;

/// Stable anchors for the game-screen title layout.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum MainMenuLayoutRole {
    TitleStage,
    ButtonPanel,
}

/// What a menu button does when pressed. Attach it next to [`Button`] and the
/// generic [`handle_menu_actions`] system takes care of the rest; no
/// per-button system needed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Start a new game (transition to [`GameState::CharacterCreation`]).
    NewGame,
    /// Resume the saved run: restore every run resource from the save and
    /// enter [`GameState::Fight`]. Only spawned when a valid save loads.
    Continue,
    /// Open the settings overlay (#30) on top of the menu.
    Settings,
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
        app.add_message::<FlowIntent>()
            .add_plugins(crate::ui_widgets::ScrollInputPlugin)
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(
                Update,
                (
                    handle_menu_actions.in_set(crate::flow::FlowIntentEmission),
                    update_button_backgrounds,
                    crate::ui_widgets::scroll_with_wheel_and_touch,
                )
                    .run_if(in_state(GameState::MainMenu)),
            )
            .add_systems(
                OnExit(GameState::MainMenu),
                despawn_screen::<MainMenuScreen>,
            );
    }
}

/// Spawns the main menu. **Continuă** is enabled exactly when a valid save
/// loads from the [`SaveStore`]; a corrupt or version-mismatched save is
/// discarded by [`load_save`] and the button stays a disabled marker.
fn spawn_main_menu(
    mut commands: Commands,
    store: Option<Res<SaveStore>>,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
) {
    let has_save = store.is_some_and(|store| load_save(&store).is_some());
    commands
        .spawn((
            MainMenuScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(24.0),
                row_gap: Val::Px(16.0),
                padding: UiRect::all(Val::Px(MENU_ROOT_PADDING)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(ARENA_BROWN),
            ScrollPosition::default(),
            crate::ui_widgets::Scrollable,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    panel_bundle(
                        &panel_texture,
                        Node {
                            width: Val::Px(MENU_TITLE_STAGE_WIDTH),
                            max_width: Val::Percent(100.0),
                            min_height: Val::Px(430.0),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::all(Val::Px(28.0)),
                            ..default()
                        },
                    ),
                    BackgroundColor(PANEL_LINEN),
                    MainMenuLayoutRole::TitleStage,
                ))
                .with_children(|stage| {
                    stage.spawn(motif_divider(&ui_font));
                    stage.spawn((
                        Text::new("Romanian Folk Fight"),
                        ui_font.text_font_bold(38.0),
                        TextColor(CREAM),
                    ));
                    stage.spawn((
                        Text::new("Basm. Port. Luptă."),
                        ui_font.text_font(19.0),
                        TextColor(CREAM),
                    ));
                    stage.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(190.0),
                            border: UiRect::all(Val::Px(2.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(WALNUT),
                        BorderColor::all(GOLD),
                        children![(
                            Text::new("*  *  *"),
                            ui_font.text_font_bold(42.0),
                            TextColor(GOLD),
                        )],
                    ));
                    stage.spawn(motif_divider(&ui_font));
                });

            parent
                .spawn((
                    panel_bundle(
                        &panel_texture,
                        Node {
                            width: Val::Px(MENU_BUTTON_PANEL_WIDTH),
                            max_width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(14.0),
                            padding: UiRect::all(Val::Px(24.0)),
                            ..default()
                        },
                    ),
                    BackgroundColor(PANEL_LINEN),
                    MainMenuLayoutRole::ButtonPanel,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        menu_button("Luptă nouă", CREAM, BUTTON_NORMAL, &ui_font),
                        MenuAction::NewGame,
                    ));
                    if has_save {
                        panel.spawn((
                            menu_button("Continuă", CREAM, BUTTON_NORMAL, &ui_font),
                            MenuAction::Continue,
                        ));
                    } else {
                        // No (valid) save to resume: a greyed-out, inert marker.
                        panel.spawn((
                            menu_button("Continuă", TEXT_DISABLED, BUTTON_DISABLED, &ui_font),
                            DisabledButton,
                        ));
                    }
                    panel.spawn((
                        menu_button("Setări", CREAM, BUTTON_NORMAL, &ui_font),
                        MenuAction::Settings,
                    ));
                    #[cfg(not(target_arch = "wasm32"))]
                    panel.spawn((
                        menu_button("Ieși", CREAM, BUTTON_NORMAL, &ui_font),
                        MenuAction::Quit,
                    ));
                });
        });
}

/// A thin gold rule flanked by two diamonds — the "motif divider" framing the
/// menu title, echoing the embroidered ii cross-stitch used on the panel
/// border.
fn motif_divider(ui_font: &UiFont) -> impl Bundle {
    (
        Text::new("* -- * -- *"),
        ui_font.text_font(20.0),
        TextColor(GOLD),
    )
}

/// A menu button with a centered text label.
fn menu_button(label: &str, text_color: Color, background: Color, ui_font: &UiFont) -> impl Bundle {
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
            ui_font.text_font(24.0),
            TextColor(text_color),
        )],
    )
}

#[cfg(test)]
fn menu_panels_fit_width(viewport_width: f32) -> bool {
    let usable_width = viewport_width - MENU_ROOT_PADDING * 2.0;
    MENU_TITLE_STAGE_WIDTH.min(usable_width) <= usable_width
        && MENU_BUTTON_PANEL_WIDTH.min(usable_width) <= usable_width
}

/// Query filter: buttons whose interaction changed this frame.
type ChangedButton = (Changed<Interaction>, With<Button>);

/// Query filter: like [`ChangedButton`], but skipping disabled buttons.
type ChangedEnabledButton = (Changed<Interaction>, With<Button>, Without<DisabledButton>);

/// Generic click handler: runs the [`MenuAction`] of whichever button was
/// pressed. Disabled buttons never carry a `MenuAction`, so they are ignored.
/// **Continuă** re-loads the save on the click (never trusting a stale
/// button) and restores every run resource. Navigation itself is not decided
/// here: `NewGame` and `Continue` apply their domain side effect (run reset,
/// save restore) first, then emit a [`FlowIntent`] so [`crate::flow`]'s
/// single transition table is the only writer of `NextState<GameState>`.
fn handle_menu_actions(
    mut commands: Commands,
    interactions: Query<(&Interaction, &MenuAction), ChangedButton>,
    store: Option<Res<SaveStore>>,
    mut flow_intents: MessageWriter<FlowIntent>,
    #[cfg(not(target_arch = "wasm32"))] mut app_exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            MenuAction::NewGame => {
                crate::progression::reset_run(&mut commands);
                flow_intents.write(FlowIntent::StartNewGame);
            }
            MenuAction::Continue => {
                let save = store.as_ref().and_then(|store| load_save(store));
                match save {
                    Some(save) => {
                        save.restore(&mut commands);
                        flow_intents.write(FlowIntent::ContinueRun);
                    }
                    None => warn!("Continuă pressed but no valid save loads; staying on the menu"),
                }
            }
            MenuAction::Settings => commands.insert_resource(SettingsOpen),
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

    /// Builds the app and settles it past `GameState::Loading` into
    /// `MainMenu` before handing it back, so every test starts with the real
    /// menu already spawned — same as under the old `#[default] MainMenu`
    /// state, just requiring the two updates the headless fall-through now
    /// takes (#114): one for `PreStartup` to run and the fall-through
    /// transition to be queued, one for it to apply and `OnEnter(MainMenu)`
    /// to spawn the screen.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            crate::flow::FlowPlugin,
            MenuPlugin,
        ));
        app.update();
        app.update();
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
    fn new_game_resets_the_previous_run_resources_before_creation() {
        let mut app = test_app();
        app.insert_resource(crate::progression::Wallet(210));
        app.insert_resource(crate::progression::Level {
            level: 3,
            xp: 90,
            unspent_points: 2,
        });
        app.insert_resource(PlayerCharacter {
            name: "Greuceanu".to_string(),
            attributes: Attributes {
                putere: 6,
                agilitate: 2,
                vitalitate: 4,
                noroc: 2,
            },
            appearance: crate::character::PlayerAppearance::default(),
        });
        app.update();
        app.world_mut()
            .spawn((Button, Interaction::Pressed, MenuAction::NewGame));
        app.update();

        assert_eq!(
            *app.world().resource::<crate::progression::Wallet>(),
            crate::progression::Wallet::default()
        );
        assert_eq!(
            *app.world().resource::<crate::progression::Level>(),
            crate::progression::Level::default()
        );
        assert!(app.world().get_resource::<PlayerCharacter>().is_none());
    }

    #[test]
    fn menu_spawns_on_enter_and_despawns_fully_on_new_game() {
        let mut app = test_app();
        app.update();
        assert_eq!(count::<MainMenuScreen>(&mut app), 1, "menu root spawned");
        let expected_buttons = if cfg!(target_arch = "wasm32") { 3 } else { 4 };
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
    fn menu_uses_separate_title_stage_and_command_panel() {
        let mut app = test_app();
        app.update();

        let roles: Vec<MainMenuLayoutRole> = app
            .world_mut()
            .query::<&MainMenuLayoutRole>()
            .iter(app.world())
            .copied()
            .collect();
        assert!(roles.contains(&MainMenuLayoutRole::TitleStage));
        assert!(roles.contains(&MainMenuLayoutRole::ButtonPanel));
        assert!(menu_panels_fit_width(375.0));

        let scroll_roots = app
            .world_mut()
            .query_filtered::<(), (With<MainMenuScreen>, With<crate::ui_widgets::Scrollable>)>()
            .iter(app.world())
            .count();
        assert_eq!(scroll_roots, 1, "narrow stacked menu can scroll");
    }

    #[test]
    fn pressing_setari_inserts_the_settings_open_marker() {
        let mut app = test_app();
        app.update();
        app.world_mut()
            .spawn((Button, Interaction::Pressed, MenuAction::Settings));
        app.update();
        assert!(
            app.world().get_resource::<SettingsOpen>().is_some(),
            "Setări opens the settings overlay without leaving the menu"
        );
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu,
            "settings is an overlay, not a state change"
        );
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

    // --- Continuă / save-load integration ---

    use crate::character::Attributes;
    use crate::creation::PlayerCharacter;
    use crate::items::ItemId;
    use crate::progression::{Level, Wallet};
    use crate::roster::LadderProgress;
    use crate::save::SaveGame;
    use crate::shop::{OwnedItems, PlayerEquipment};
    use std::collections::HashSet;

    /// A valid mid-run save as stored JSON.
    fn saved_run_json() -> String {
        let player = PlayerCharacter {
            name: "Greuceanu".to_string(),
            attributes: Attributes {
                putere: 6,
                agilitate: 2,
                vitalitate: 4,
                noroc: 2,
            },
            appearance: crate::character::PlayerAppearance::default(),
        };
        let level = Level {
            level: 3,
            xp: 40,
            unspent_points: 1,
        };
        let mut equipment = crate::items::Equipment::default();
        equipment.equip(ItemId::ToporDePadurar);
        SaveGame::capture(
            &player,
            &level,
            &Wallet(210),
            &OwnedItems(HashSet::from([ItemId::ToporDePadurar])),
            &PlayerEquipment(equipment),
            &LadderProgress(4),
        )
        .to_json()
        .expect("plain data serializes")
    }

    /// The menu app over an in-memory save store seeded with `json`. Builds
    /// its own app (rather than calling [`test_app`]) so the store is
    /// inserted *before* the menu ever spawns: [`test_app`] now settles all
    /// the way to `MainMenu` on its own (#114), which would spawn the menu
    /// (and lock in `Continuă`'s enabled/disabled state) before this
    /// function got a chance to insert the store it depends on.
    fn test_app_with_save(
        json: Option<&str>,
    ) -> (App, std::sync::Arc<std::sync::Mutex<Option<String>>>) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            crate::flow::FlowPlugin,
            MenuPlugin,
        ));
        let (store, cell) = SaveStore::in_memory();
        if let Some(json) = json {
            store.store(json);
        }
        app.insert_resource(store);
        app.update(); // PreStartup runs; no `AssetServer` -> fall-through queued.
        app.update(); // fall-through applies; `OnEnter(MainMenu)` spawns the menu.
        (app, cell)
    }

    fn continue_button(app: &mut App) -> Option<Entity> {
        app.world_mut()
            .query_filtered::<(Entity, &MenuAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &action)| action == MenuAction::Continue)
            .map(|(entity, _)| entity)
    }

    #[test]
    fn a_valid_save_enables_continua() {
        let (mut app, _cell) = test_app_with_save(Some(&saved_run_json()));
        let button = continue_button(&mut app).expect("Continuă carries MenuAction::Continue");
        assert!(
            !app.world().entity(button).contains::<DisabledButton>(),
            "the resumable button is not a disabled marker"
        );
        assert_eq!(count::<DisabledButton>(&mut app), 0);
    }

    #[test]
    fn pressing_continua_restores_the_run_and_enters_the_fight() {
        let (mut app, _cell) = test_app_with_save(Some(&saved_run_json()));
        let button = continue_button(&mut app).expect("Continuă is enabled");
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update(); // handler restores + queues the transition
        app.update(); // transition applies

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Fight,
            "Continuă resumes straight into the fight"
        );
        let player = app.world().resource::<PlayerCharacter>();
        assert_eq!(player.name, "Greuceanu");
        assert_eq!(
            player.attributes,
            Attributes {
                putere: 6,
                agilitate: 2,
                vitalitate: 4,
                noroc: 2,
            }
        );
        assert_eq!(
            *app.world().resource::<Level>(),
            Level {
                level: 3,
                xp: 40,
                unspent_points: 1,
            }
        );
        assert_eq!(*app.world().resource::<Wallet>(), Wallet(210));
        assert_eq!(
            *app.world().resource::<OwnedItems>(),
            OwnedItems(HashSet::from([ItemId::ToporDePadurar]))
        );
        assert_eq!(
            app.world()
                .resource::<PlayerEquipment>()
                .0
                .equipped(crate::items::Slot::Weapon),
            Some(ItemId::ToporDePadurar)
        );
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress(4),
            "the run resumes on the saved opponent"
        );
    }

    #[test]
    fn a_corrupt_save_keeps_continua_disabled_and_clears_the_store() {
        let (mut app, cell) = test_app_with_save(Some("garbage, not JSON"));
        assert!(
            continue_button(&mut app).is_none(),
            "no resumable button for a corrupt save"
        );
        assert_eq!(count::<DisabledButton>(&mut app), 1, "greyed-out marker");
        assert_eq!(
            *cell.lock().expect("test store lock"),
            None,
            "the corrupt save is cleared on the menu load"
        );
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu
        );
    }

    #[test]
    fn an_empty_store_keeps_continua_disabled() {
        let (mut app, _cell) = test_app_with_save(None);
        assert!(continue_button(&mut app).is_none());
        assert_eq!(count::<DisabledButton>(&mut app), 1);
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
