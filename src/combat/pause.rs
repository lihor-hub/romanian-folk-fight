//! Pause overlay for the fight screen: a [`SubStates`] scoped to
//! `GameState::Fight`, an Esc / ⏸-button toggle, and a scrim-plus-panel
//! overlay with resume / settings / abandon actions.
//!
//! Combat input and the enemy reply are gated on [`PauseState::Running`] via
//! run conditions in [`super::systems::CombatPlugin`], so pausing freezes the
//! duel without touching any combat state: the turn, HP, stamina, RNG, and
//! log all survive a pause round-trip untouched.

use bevy::prelude::*;

use crate::core::{GameState, UiFont, despawn_screen};
use crate::menu::DisabledButton;
use crate::settings::SettingsOpen;
use crate::theme::{
    BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, PanelTexture, SCRIM, panel_bundle,
};

/// Whether the running fight is paused. Exists only inside
/// `GameState::Fight`; leaving the fight drops it (and the overlay with it).
#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(GameState = GameState::Fight)]
pub enum PauseState {
    #[default]
    Running,
    Paused,
}

/// Marker for the pause-overlay root; despawned on unpause or on leaving the
/// fight.
#[derive(Component)]
pub(super) struct PauseOverlay;

/// The small ⏸ button top-center of the combat HUD.
#[derive(Component)]
pub(super) struct PauseButton;

/// What an overlay button does when clicked.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PauseAction {
    /// «Continuă lupta» — unpause and resume the same turn.
    Resume,
    /// «Setări» — open the settings overlay on top of the pause panel;
    /// closing it returns here, still paused.
    Settings,
    /// «Abandonează» — back to the main menu; the run keeps its last
    /// autosave, and the fight restarts fresh on return.
    Abandon,
}

pub(super) struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<PauseState>()
            .add_systems(OnEnter(PauseState::Paused), spawn_overlay)
            .add_systems(OnExit(PauseState::Paused), despawn_screen::<PauseOverlay>)
            .add_systems(
                Update,
                (
                    toggle_on_esc.run_if(not(resource_exists::<SettingsOpen>)),
                    handle_pause_button,
                    handle_overlay_buttons,
                    update_button_backgrounds,
                )
                    .run_if(in_state(GameState::Fight)),
            );
    }
}

/// Esc toggles the pause overlay while fighting. Gated off while the
/// settings overlay sits on top (see the plugin's run condition): Esc must
/// not silently unpause the fight under it.
fn toggle_on_esc(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<PauseState>>,
    mut next: ResMut<NextState<PauseState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(match state.get() {
            PauseState::Running => PauseState::Paused,
            PauseState::Paused => PauseState::Running,
        });
    }
}

/// The HUD ⏸ button opens the overlay (touch-friendly counterpart to Esc).
fn handle_pause_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<PauseButton>)>,
    mut next: ResMut<NextState<PauseState>>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            next.set(PauseState::Paused);
        }
    }
}

/// Query filter: enabled overlay buttons whose interaction changed this
/// frame (the HUD's `ChangedEnabledButton` shape, scoped to the overlay).
type ChangedEnabledOverlayButton = (Changed<Interaction>, With<Button>, Without<DisabledButton>);

/// Query filter: any enabled pause-related button (the HUD ⏸ or an overlay
/// button) whose interaction changed this frame.
type ChangedEnabledPauseButton = (
    Changed<Interaction>,
    Or<(With<PauseAction>, With<PauseButton>)>,
    Without<DisabledButton>,
);

/// Applies the clicked overlay action: resume the duel or abandon to the
/// main menu. Abandoning is not a defeat and never touches the save — the
/// run keeps its last autosave and the fight restarts on return.
fn handle_overlay_buttons(
    mut commands: Commands,
    interactions: Query<(&Interaction, &PauseAction), ChangedEnabledOverlayButton>,
    mut next_pause: ResMut<NextState<PauseState>>,
    mut next_game: ResMut<NextState<GameState>>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            PauseAction::Resume => next_pause.set(PauseState::Running),
            PauseAction::Settings => commands.insert_resource(SettingsOpen),
            PauseAction::Abandon => next_game.set(GameState::MainMenu),
        }
    }
}

/// Hover/pressed feedback for the enabled pause-related buttons (the same
/// pattern as the menu and HUD, scoped to this overlay's buttons).
fn update_button_backgrounds(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedEnabledPauseButton>,
) {
    for (interaction, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None => BUTTON_NORMAL,
        };
    }
}

/// Spawns the semi-transparent scrim and the pause panel.
fn spawn_overlay(mut commands: Commands, ui_font: Res<UiFont>, panel_texture: Res<PanelTexture>) {
    commands.spawn((
        PauseOverlay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(SCRIM),
        // Above the HUD, and the scrim swallows clicks aimed at it.
        GlobalZIndex(10),
        children![(
            panel_bundle(
                &panel_texture,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(14.0),
                    padding: UiRect::all(Val::Px(28.0)),
                    ..default()
                },
            ),
            children![
                (
                    Text::new("Pauză"),
                    ui_font.text_font(34.0),
                    TextColor(CREAM),
                ),
                overlay_button("Continuă lupta", PauseAction::Resume, &ui_font),
                overlay_button("Setări", PauseAction::Settings, &ui_font),
                overlay_button("Abandonează", PauseAction::Abandon, &ui_font),
            ],
        )],
    ));
}

/// One wide, enabled overlay button in the main-menu style.
fn overlay_button(label: &str, action: PauseAction, ui_font: &UiFont) -> impl Bundle {
    button_parts(label, action, BUTTON_NORMAL, CREAM, ui_font)
}

/// The shared shape of an overlay button: wide, centered label.
fn button_parts(
    label: &str,
    action: PauseAction,
    background: Color,
    text: Color,
    ui_font: &UiFont,
) -> impl Bundle {
    (
        Button,
        action,
        Node {
            width: Val::Px(260.0),
            height: Val::Px(56.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(background),
        children![(Text::new(label), ui_font.text_font(24.0), TextColor(text),)],
    )
}

#[cfg(test)]
mod tests {
    use super::super::systems::{
        CombatPlugin, CombatRng, CombatSide, CombatTurn, PlayerActionEvent,
    };
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::character::{Attributes, EnemyFighter, Health, PlayerFighter, Stamina};
    use crate::combat::engine::CombatAction;
    use crate::core::CorePlugin;
    use crate::creation::PlayerCharacter;
    use crate::save::SaveRequested;
    use bevy::state::app::StatesPlugin;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    /// Headless app already on the fight screen with a fixed duel RNG.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin));
        app.add_message::<SaveRequested>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes: PLAYER_ATTRIBUTES,
            appearance: crate::character::PlayerAppearance::default(),
        });
        app.insert_resource(CombatRng(ChaCha8Rng::seed_from_u64(7)));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update(); // transition + OnEnter + first combat frame
        app
    }

    /// Presses Esc for one frame, then runs a second frame so the queued
    /// state transition applies (the keys are cleared first: without the
    /// input plugin, `just_pressed` would otherwise re-toggle).
    fn press_esc(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::Escape);
        keys.clear();
        app.update();
    }

    fn pause_state(app: &App) -> PauseState {
        *app.world().resource::<State<PauseState>>().get()
    }

    fn game_state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    fn overlay_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<PauseOverlay>>()
            .iter(app.world())
            .count()
    }

    fn find_overlay_button(app: &mut App, label: &str) -> Entity {
        let children: Vec<(Entity, Vec<Entity>)> = app
            .world_mut()
            .query_filtered::<(Entity, &Children), (With<Button>, With<PauseAction>)>()
            .iter(app.world())
            .map(|(e, c)| (e, c.iter().collect()))
            .collect();
        for (button, kids) in children {
            for kid in kids {
                if let Some(text) = app.world().get::<Text>(kid)
                    && text.0 == label
                {
                    return button;
                }
            }
        }
        panic!("overlay button «{label}» exists");
    }

    fn click(app: &mut App, button: Entity) {
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.world_mut().entity_mut(button).insert(Interaction::None);
        app.update();
    }

    fn combat_snapshot(app: &mut App) -> (CombatTurn, (i32, i32), (i32, i32)) {
        let turn = *app.world().resource::<CombatTurn>();
        let player = pools::<PlayerFighter>(app);
        let enemy = pools::<EnemyFighter>(app);
        (turn, player, enemy)
    }

    fn pools<M: Component>(app: &mut App) -> (i32, i32) {
        let (health, stamina) = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<M>>()
            .single(app.world())
            .expect("fighter exists");
        (health.current, stamina.current)
    }

    #[test]
    fn the_fight_starts_running_with_no_overlay() {
        let mut app = test_app();
        assert_eq!(pause_state(&app), PauseState::Running);
        assert_eq!(overlay_count(&mut app), 0);
    }

    #[test]
    fn esc_toggles_the_overlay_on_and_off() {
        let mut app = test_app();
        press_esc(&mut app);
        assert_eq!(pause_state(&app), PauseState::Paused);
        assert_eq!(overlay_count(&mut app), 1, "scrim spawned");
        press_esc(&mut app);
        assert_eq!(pause_state(&app), PauseState::Running);
        assert_eq!(overlay_count(&mut app), 0, "scrim despawned");
    }

    #[test]
    fn the_hud_pause_button_opens_the_overlay() {
        let mut app = test_app();
        let button = app
            .world_mut()
            .query_filtered::<Entity, (With<Button>, With<PauseButton>)>()
            .single(app.world())
            .expect("the HUD carries one pause button");
        click(&mut app, button);
        assert_eq!(pause_state(&app), PauseState::Paused);
    }

    #[test]
    fn actions_written_while_paused_resolve_nothing() {
        let mut app = test_app();
        assert_eq!(
            app.world().resource::<CombatTurn>().side,
            CombatSide::Player,
            "agility tie opens with the player"
        );
        let before = combat_snapshot(&mut app);
        press_esc(&mut app);

        // Keyboard and HUD both funnel into this message; write it directly.
        app.world_mut()
            .write_message(PlayerActionEvent(CombatAction::QuickStrike));
        app.update();
        app.update();
        assert_eq!(
            combat_snapshot(&mut app),
            before,
            "no engine resolution while paused"
        );
    }

    #[test]
    fn a_pause_round_trip_preserves_the_exact_combat_state() {
        let mut app = test_app();
        let before = combat_snapshot(&mut app);
        let rng_before = app.world().resource::<CombatRng>().0.clone();
        press_esc(&mut app);
        app.update();
        press_esc(&mut app);
        app.update();
        assert_eq!(combat_snapshot(&mut app), before, "turn, HP, stamina kept");
        assert_eq!(
            app.world().resource::<CombatRng>().0,
            rng_before,
            "no combat roll happened while paused"
        );
    }

    #[test]
    fn continua_lupta_unpauses() {
        let mut app = test_app();
        press_esc(&mut app);
        let button = find_overlay_button(&mut app, "Continuă lupta");
        click(&mut app, button);
        assert_eq!(pause_state(&app), PauseState::Running);
        assert_eq!(overlay_count(&mut app), 0);
    }

    #[test]
    fn setari_opens_the_settings_overlay_and_stays_paused() {
        let mut app = test_app();
        press_esc(&mut app);
        let button = find_overlay_button(&mut app, "Setări");
        assert!(
            !app.world().entity(button).contains::<DisabledButton>(),
            "Setări is enabled now that the settings overlay exists"
        );
        click(&mut app, button);
        assert!(
            app.world().get_resource::<SettingsOpen>().is_some(),
            "Setări opens the settings overlay"
        );
        assert_eq!(pause_state(&app), PauseState::Paused, "still paused");
        assert_eq!(game_state(&app), GameState::Fight);
    }

    #[test]
    fn esc_is_inert_while_the_settings_overlay_is_open() {
        let mut app = test_app();
        press_esc(&mut app);
        assert_eq!(pause_state(&app), PauseState::Paused);
        app.insert_resource(SettingsOpen);
        app.update();
        press_esc(&mut app);
        assert_eq!(
            pause_state(&app),
            PauseState::Paused,
            "Esc must not unpause the fight under the settings overlay"
        );
    }

    #[test]
    fn abandoneaza_returns_to_the_menu_without_touching_the_save() {
        let mut app = test_app();
        press_esc(&mut app);
        let button = find_overlay_button(&mut app, "Abandonează");
        click(&mut app, button);
        assert_eq!(game_state(&app), GameState::MainMenu);
        assert_eq!(overlay_count(&mut app), 0, "overlay gone with the fight");
        let saves = app
            .world_mut()
            .resource_mut::<Messages<SaveRequested>>()
            .drain()
            .count();
        assert_eq!(saves, 0, "abandoning requests no save");
    }

    #[test]
    fn leaving_the_fight_drops_the_pause_substate() {
        let mut app = test_app();
        press_esc(&mut app);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::MainMenu);
        app.update();
        assert!(
            app.world().get_resource::<State<PauseState>>().is_none(),
            "the substate exists only inside the fight"
        );
        assert_eq!(overlay_count(&mut app), 0);
    }
}
