//! ECS glue for the combat engine: the turn resource, the seeded RNG
//! resource, the HUD input hookup (plus a debug-only 1–4 keyboard mapping),
//! the AI-driven enemy reply, and the write-back of
//! [`engine::resolve_action`] results onto `Health` and `Stamina` components.

use std::time::Duration;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::character::{Attributes, EnemyFighter, Health, PlayerFighter, Stamina};
use crate::core::{GameState, despawn_screen};
use crate::items::Equipment;
use crate::ui_widgets::focus::{FocusNavigationPlugin, FocusNavigationSet};

use super::action_palette;
use super::actions::ExtraDescriptors;
use super::ai::{self, AiProfile};
use super::engine::{self, CombatAction, CombatEvent, DuelDistance};
use super::hud;
use super::pause::{self, PauseState};

/// Unity-style combat cooldown: slightly longer than the current 0.4s attack
/// clip, so a readable pose lands before the next fighter acts.
pub const PRESENTATION_DELAY_SECONDS: f32 = 0.5;

/// The two sides of a duel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatSide {
    Player,
    Enemy,
}

impl CombatSide {
    /// The other side of the duel.
    pub fn opponent(self) -> Self {
        match self {
            Self::Player => Self::Enemy,
            Self::Enemy => Self::Player,
        }
    }
}

/// Turn state of the running duel. Inserted once both fighters exist (the
/// faster `agilitate` opens, ties to the player) and removed when the fight
/// screen exits.
///
/// The blocking flags live here rather than on the fighters because they are
/// duel state: they expire on the owner's turn and reset between fights.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatTurn {
    /// Whose action is being awaited.
    pub side: CombatSide,
    /// Set when a fighter is defeated; all combat input stops.
    pub over: bool,
    /// Whether the player is guarding since their last turn.
    pub player_blocking: bool,
    /// Whether the enemy is guarding since their last turn.
    pub enemy_blocking: bool,
    /// Persistent relative spacing between the fighters.
    pub distance: DuelDistance,
}

/// Seeded RNG that drives every combat roll; the engine never touches
/// `thread_rng`. [`setup_combat`] seeds it from the app clock unless one was
/// already provided (tests insert a fixed seed for deterministic duels).
#[derive(Resource, Debug, Clone)]
pub struct CombatRng(pub ChaCha8Rng);

/// Presentation gate between resolved combat actions.
#[derive(Resource, Debug, Clone, Default)]
pub struct CombatPresentation {
    timer: Option<Timer>,
}

impl CombatPresentation {
    fn start(&mut self) {
        self.timer = Some(Timer::new(
            Duration::from_secs_f32(PRESENTATION_DELAY_SECONDS),
            TimerMode::Once,
        ));
    }

    /// Whether the presentation window is still blocking the next action.
    pub fn is_busy(&self) -> bool {
        self.timer
            .as_ref()
            .is_some_and(|timer| !timer.is_finished())
    }

    fn tick(&mut self, delta: Duration) {
        let Some(timer) = self.timer.as_mut() else {
            return;
        };
        timer.tick(delta);
        if timer.is_finished() {
            self.timer = None;
        }
    }
}

/// The player's chosen action for this turn. Written by the HUD action
/// buttons (and, in debug builds, the 1–4 keyboard mapping).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerActionEvent(pub CombatAction);

/// A combat event that occurred, tagged with who acted. The HUD log,
/// announcer, and FX issues consume these.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatLogEvent {
    /// Who performed the action that produced this event.
    pub actor: CombatSide,
    /// The action the actor performed (the arena FX shake heavy strikes
    /// differently from regular ones).
    pub action: CombatAction,
    /// What happened.
    pub event: CombatEvent,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(pause::PausePlugin)
            // #213: shared keyboard/gamepad focus navigation — see
            // `crate::ui_widgets::focus`'s module docs. `handle_action_buttons`/
            // `handle_category_buttons` below are ordered `.after(FocusNavigationSet)`
            // so a same-frame Enter/gamepad-South press (which sets
            // `Interaction::Pressed`, see `activate_focused_control`) is
            // observed by them this same `Update` pass, not one frame later —
            // the same reasoning `crate::flow::FlowIntentEmission` documents.
            .add_plugins(FocusNavigationPlugin)
            .init_resource::<ExtraDescriptors>()
            .init_resource::<action_palette::PhonePaletteState>()
            .init_resource::<action_palette::ActionPictograms>()
            .add_systems(Startup, action_palette::load_action_pictograms)
            .add_message::<PlayerActionEvent>()
            .add_message::<CombatLogEvent>()
            .add_systems(OnEnter(GameState::Fight), (setup_combat, hud::spawn_hud))
            .add_systems(
                OnExit(GameState::Fight),
                (
                    teardown_combat,
                    hud::teardown_hud,
                    despawn_screen::<hud::HudScreen>,
                ),
            )
            .add_systems(
                Update,
                (
                    init_turn,
                    // Combat input and the enemy reply freeze while paused
                    // (run condition, not per-system ifs); the HUD display
                    // systems below keep running under the overlay.
                    (
                        tick_presentation,
                        action_palette::handle_action_buttons.after(FocusNavigationSet),
                        resolve_player_action,
                        enemy_turn,
                    )
                        .chain()
                        .run_if(in_state(PauseState::Running)),
                    hud::collect_log_lines,
                    action_palette::update_button_backgrounds,
                    action_palette::update_action_buttons,
                    action_palette::pulse_distance_chip_on_reach_hover,
                    hud::apply_responsive_hud_layout,
                    action_palette::handle_category_buttons.after(FocusNavigationSet),
                    action_palette::rebuild_action_bar_on_breakpoint_change,
                    action_palette::sync_phone_open_category,
                    action_palette::update_category_button_backgrounds,
                    hud::apply_letterbox_to_hud_root,
                    (
                        hud::update_bar_fills,
                        hud::update_labels,
                        hud::update_log_text,
                        hud::sync_hud_palette,
                        hud::sync_hud_panel_alpha,
                    ),
                )
                    .chain()
                    .run_if(in_state(GameState::Fight)),
            );
        // The pre-HUD keyboard mapping stays as a debug convenience only;
        // release builds are mouse-driven through the HUD.
        #[cfg(debug_assertions)]
        app.add_systems(
            Update,
            player_input
                .after(init_turn)
                .after(tick_presentation)
                .before(resolve_player_action)
                .run_if(in_state(GameState::Fight))
                .run_if(in_state(PauseState::Running)),
        );
    }
}

/// Query for the components the resolver reads and writes on one side.
type FighterComponents<'w, 's, Side, OtherSide> = Query<
    'w,
    's,
    (
        &'static Attributes,
        &'static Equipment,
        &'static mut Health,
        &'static mut Stamina,
    ),
    (With<Side>, Without<OtherSide>),
>;
type PlayerQuery<'w, 's> = FighterComponents<'w, 's, PlayerFighter, EnemyFighter>;
type EnemyQuery<'w, 's> = FighterComponents<'w, 's, EnemyFighter, PlayerFighter>;

/// Seeds the duel RNG from the app clock — unless a [`CombatRng`] already
/// exists, so tests (or a future daily-seed mode) can provide their own.
/// Also resets the phone palette's category disclosure (#199) so every
/// fight starts with no category open, even if the player left the previous
/// fight mid-disclosure, and (#213) clears keyboard/gamepad focus so a
/// previous fight's button entity (now despawned) is never left named by
/// [`InputFocus`].
fn setup_combat(
    mut commands: Commands,
    time: Res<Time>,
    rng: Option<Res<CombatRng>>,
    mut input_focus: ResMut<InputFocus>,
) {
    if rng.is_none() {
        commands.insert_resource(CombatRng(ChaCha8Rng::seed_from_u64(
            time.elapsed().as_micros() as u64,
        )));
    }
    commands.init_resource::<CombatPresentation>();
    commands.insert_resource(action_palette::PhonePaletteState::default());
    input_focus.clear();
}

/// Drops the duel state so the next fight starts fresh.
fn teardown_combat(mut commands: Commands) {
    commands.remove_resource::<CombatTurn>();
    commands.remove_resource::<CombatRng>();
    commands.remove_resource::<CombatPresentation>();
}

/// Advances the presentation gate before action systems decide whether the
/// next fighter may act.
fn tick_presentation(time: Res<Time>, presentation: Option<ResMut<CombatPresentation>>) {
    if let Some(mut presentation) = presentation {
        presentation.tick(time.delta());
    }
}

/// Inserts [`CombatTurn`] once both fighters exist (they are spawned by the
/// arena's own `OnEnter` system): the fighter with higher `agilitate` opens,
/// ties go to the player.
fn init_turn(
    mut commands: Commands,
    turn: Option<Res<CombatTurn>>,
    player: Query<&Attributes, With<PlayerFighter>>,
    enemy: Query<&Attributes, (With<EnemyFighter>, Without<PlayerFighter>)>,
) {
    if turn.is_some() {
        return;
    }
    let (Ok(player), Ok(enemy)) = (player.single(), enemy.single()) else {
        return;
    };
    let side = if engine::player_acts_first(player, enemy) {
        CombatSide::Player
    } else {
        CombatSide::Enemy
    };
    commands.insert_resource(CombatTurn {
        side,
        over: false,
        player_blocking: false,
        enemy_blocking: false,
        distance: DuelDistance::starting(),
    });
}

/// Debug-only keyboard mapping (the HUD buttons are the real input): 1–4 are
/// strikes/guard/rest, 5–7 are movement, and 8 is the normal strike
/// (appended so the long-standing 1–7 bindings keep their meanings). Only
/// listens on the player's turn while the duel is running.
#[cfg(debug_assertions)]
fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    turn: Option<Res<CombatTurn>>,
    presentation: Option<Res<CombatPresentation>>,
    mut actions: MessageWriter<PlayerActionEvent>,
) {
    let Some(turn) = turn else {
        return;
    };
    if presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy)
    {
        return;
    }
    if turn.side != CombatSide::Player || turn.over {
        return;
    }
    let mappings = [
        (KeyCode::Digit1, CombatAction::QuickStrike),
        (KeyCode::Digit2, CombatAction::HeavyStrike),
        (KeyCode::Digit3, CombatAction::Block),
        (KeyCode::Digit4, CombatAction::Rest),
        (KeyCode::Digit5, CombatAction::StepForward),
        (KeyCode::Digit6, CombatAction::StepBack),
        (KeyCode::Digit7, CombatAction::LeapForward),
        (KeyCode::Digit8, CombatAction::NormalStrike),
    ];
    for (key, action) in mappings {
        if keys.just_pressed(key) {
            actions.write(PlayerActionEvent(action));
            return;
        }
    }
}

/// Applies the player's chosen action; any extra queued actions this turn are
/// dropped.
fn resolve_player_action(
    mut actions: MessageReader<PlayerActionEvent>,
    turn: Option<ResMut<CombatTurn>>,
    rng: Option<ResMut<CombatRng>>,
    presentation: Option<ResMut<CombatPresentation>>,
    mut log: MessageWriter<CombatLogEvent>,
    mut player: PlayerQuery,
    mut enemy: EnemyQuery,
) {
    let (Some(mut turn), Some(mut rng)) = (turn, rng) else {
        return;
    };
    let mut presentation = presentation;
    if presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy)
    {
        for _ in actions.read() {}
        return;
    }
    for PlayerActionEvent(action) in actions.read().copied() {
        if turn.side != CombatSide::Player || turn.over {
            continue;
        }
        let (Ok(player), Ok(enemy)) = (player.single_mut(), enemy.single_mut()) else {
            return;
        };
        apply_action(
            action,
            CombatSide::Player,
            player,
            enemy,
            &mut turn,
            &mut rng,
            &mut log,
        );
        if let Some(presentation) = presentation.as_deref_mut() {
            presentation.start();
        }
    }
}

/// The enemy reply, chosen by [`ai::choose_action`] from snapshots of both
/// fighters and the enemy's [`AiProfile`] (default aggression if the spawner
/// did not attach one). Waits for the presentation gate after player actions,
/// while drawing from the same seeded RNG as the resolver.
fn enemy_turn(
    turn: Option<ResMut<CombatTurn>>,
    rng: Option<ResMut<CombatRng>>,
    presentation: Option<ResMut<CombatPresentation>>,
    mut log: MessageWriter<CombatLogEvent>,
    mut player: PlayerQuery,
    mut enemy: EnemyQuery,
    profile: Query<&AiProfile, With<EnemyFighter>>,
) {
    let (Some(mut turn), Some(mut rng)) = (turn, rng) else {
        return;
    };
    if turn.side != CombatSide::Enemy || turn.over {
        return;
    }
    let mut presentation = presentation;
    if presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy)
    {
        return;
    }
    let (Ok(player), Ok(enemy)) = (player.single_mut(), enemy.single_mut()) else {
        return;
    };
    let me = snapshot(&enemy, turn.enemy_blocking);
    let foe = snapshot(&player, turn.player_blocking);
    let profile = profile.single().copied().unwrap_or_else(|error| {
        warn!("no unique enemy AiProfile ({error}); using the default");
        AiProfile::default()
    });
    let action = ai::choose_action_at_distance(&me, &foe, &profile, turn.distance, &mut rng.0);
    apply_action(
        action,
        CombatSide::Enemy,
        enemy,
        player,
        &mut turn,
        &mut rng,
        &mut log,
    );
    if let Some(presentation) = presentation.as_deref_mut() {
        presentation.start();
    }
}

/// One side's components as yielded by a [`FighterComponents`] query.
type FighterItem<'a> = (
    &'a Attributes,
    &'a Equipment,
    Mut<'a, Health>,
    Mut<'a, Stamina>,
);

/// Snapshots one side's components (plus its blocking flag from the turn
/// resource) into a pure [`engine::FighterState`], aggregating the equipped
/// gear into the flat damage/armor bonuses the engine consumes.
fn snapshot(fighter: &FighterItem, blocking: bool) -> engine::FighterState {
    let (attributes, equipment, hp, stamina) = fighter;
    engine::FighterState {
        hp: hp.current,
        stamina: stamina.current,
        attributes: **attributes,
        damage_bonus: equipment.total_damage_bonus(),
        armor: equipment.total_armor(),
        blocking,
    }
}

/// Snapshots both fighters into pure [`engine::FighterState`]s, resolves the
/// action, writes the results back to the components and the turn resource,
/// logs every event, and passes the turn — ending the duel on `Defeated`.
///
/// The turn passes even on an [`CombatEvent::OutOfStamina`] no-op so a
/// fighter can never wedge the duel by re-trying a strike forever.
fn apply_action(
    action: CombatAction,
    actor_side: CombatSide,
    actor: FighterItem,
    target: FighterItem,
    turn: &mut CombatTurn,
    rng: &mut CombatRng,
    log: &mut MessageWriter<CombatLogEvent>,
) {
    let (actor_blocking, target_blocking) = match actor_side {
        CombatSide::Player => (&mut turn.player_blocking, &mut turn.enemy_blocking),
        CombatSide::Enemy => (&mut turn.enemy_blocking, &mut turn.player_blocking),
    };
    let mut actor_state = snapshot(&actor, *actor_blocking);
    let mut target_state = snapshot(&target, *target_blocking);
    let (_, _, mut actor_hp, mut actor_stamina) = actor;
    let (_, _, mut target_hp, mut target_stamina) = target;

    let events = engine::resolve_action_at_distance(
        &mut actor_state,
        &mut target_state,
        action,
        &mut turn.distance,
        &mut rng.0,
    );

    actor_hp.current = actor_state.hp;
    actor_stamina.current = actor_state.stamina;
    target_hp.current = target_state.hp;
    target_stamina.current = target_state.stamina;
    *actor_blocking = actor_state.blocking;
    *target_blocking = target_state.blocking;
    if events.contains(&CombatEvent::Defeated) {
        turn.over = true;
    }
    for event in events {
        log.write(CombatLogEvent {
            actor: actor_side,
            action,
            event,
        });
    }
    turn.side = actor_side.opponent();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::arena::animation::FighterClip;
    use crate::character::stats::{CRIT_PERCENT_CAP, HIT_PERCENT_MIN};
    use crate::core::CorePlugin;
    use crate::creation::PlayerCharacter;
    use crate::flow::FlowPlugin;
    use crate::settings::AccessibilityPreferences;
    use bevy::state::app::StatesPlugin;
    use rand::RngExt as _;

    /// Same player build as the arena tests: putere 4 (damage 6), agilitate
    /// 2 (ties the Hoț de codru), vitalitate 4 (90 hp, 50 stamina), noroc 3.
    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
        atac: 1,
        aparare: 2,
        carisma: 1,
        magie: 0,
    };

    /// Headless app on the fight screen with a deterministic duel RNG whose
    /// first four strikes are clean hits without crits (see [`strikes_rng`]).
    fn test_app() -> App {
        test_app_with(PLAYER_ATTRIBUTES)
    }

    fn test_app_with(attributes: Attributes) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, FlowPlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(10));
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes,
            appearance: crate::character::PlayerAppearance::default(),
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
        });
        app.insert_resource(CombatRng(strikes_rng(4)));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update(); // transition + OnEnter + first combat frame
        app
    }

    /// Same fixture as [`test_app_with`], with [`AccessibilityPreferences`]
    /// pinned to a chosen `reduced_motion` value -- used by the #200 timing-
    /// invariance test below. `insert_resource` unconditionally overwrites
    /// whatever `ArenaPlugin`'s `FxPlugin`/`AnimationPlugin` already
    /// idempotently initialized, regardless of plugin registration order.
    fn test_app_with_motion(attributes: Attributes, reduced_motion: bool) -> App {
        let mut app = test_app_with(attributes);
        app.insert_resource(AccessibilityPreferences {
            reduced_motion,
            high_contrast: false,
        });
        app
    }

    /// A `ChaCha8Rng` whose first `strikes` strikes are guaranteed clean
    /// hits without crits, regardless of attributes: a landed strike
    /// consumes two `0..100` rolls (hit, then crit), so the stream must
    /// alternate below the minimum hit chance / at or above the crit cap.
    fn strikes_rng(strikes: usize) -> ChaCha8Rng {
        'seed: for seed in 0..1_000_000u64 {
            let mut probe = ChaCha8Rng::seed_from_u64(seed);
            for _ in 0..strikes {
                if probe.random_range(0..100) >= HIT_PERCENT_MIN
                    || probe.random_range(0..100) < CRIT_PERCENT_CAP
                {
                    continue 'seed;
                }
            }
            return ChaCha8Rng::seed_from_u64(seed);
        }
        panic!("no seed under 1000000 lands {strikes} clean strikes");
    }

    // The keyboard mapping (and thus every key-driven test) only exists in
    // debug builds; the HUD tests cover the same resolution paths through
    // mouse input in every profile.
    #[cfg(debug_assertions)]
    fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    fn turn(app: &App) -> CombatTurn {
        *app.world().resource::<CombatTurn>()
    }

    fn presentation_busy(app: &App) -> bool {
        app.world().resource::<CombatPresentation>().is_busy()
    }

    fn advance_presentation(app: &mut App) {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs_f32(PRESENTATION_DELAY_SECONDS + 0.1),
        ));
        app.update();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::ZERO,
        ));
    }

    fn player_pools(app: &mut App) -> (i32, i32) {
        pools::<PlayerFighter>(app)
    }

    fn enemy_pools(app: &mut App) -> (i32, i32) {
        pools::<EnemyFighter>(app)
    }

    fn pools<M: Component>(app: &mut App) -> (i32, i32) {
        let (health, stamina) = app
            .world_mut()
            .query_filtered::<(&Health, &Stamina), With<M>>()
            .single(app.world())
            .expect("fighter exists");
        (health.current, stamina.current)
    }

    fn clip<M: Component>(app: &mut App) -> FighterClip {
        *app.world_mut()
            .query_filtered::<&FighterClip, With<M>>()
            .single(app.world())
            .expect("fighter exists")
    }

    /// Drains the enemy's stamina below the quick-strike cost, forcing the
    /// AI's deterministic Rest branch (which consumes no RNG rolls) so the
    /// player-side expectations stay exact.
    #[cfg(debug_assertions)]
    fn drain_enemy_stamina(app: &mut App) {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Stamina, With<EnemyFighter>>();
        query
            .single_mut(app.world_mut())
            .expect("enemy fighter exists")
            .current = 0;
    }

    /// Presses `key` against an inert enemy: the drain must be re-applied
    /// before every press because the forced Rest refills 20 stamina.
    #[cfg(debug_assertions)]
    fn press_vs_resting_enemy(app: &mut App, key: KeyCode) {
        drain_enemy_stamina(app);
        press(app, key);
        advance_presentation(app);
        advance_presentation(app);
    }

    #[test]
    fn the_turn_opens_with_the_player_on_an_agility_tie() {
        let mut app = test_app(); // player agilitate 2 vs Hoț de codru 2
        assert_eq!(
            turn(&app),
            CombatTurn {
                side: CombatSide::Player,
                over: false,
                player_blocking: false,
                enemy_blocking: false,
                distance: DuelDistance::starting(),
            }
        );
        assert_eq!(enemy_pools(&mut app), (70, 40), "enemy untouched");
    }

    #[test]
    fn a_faster_enemy_opens_the_round_and_acts_immediately() {
        let mut app = test_app_with(Attributes {
            agilitate: 1, // the Hoț de codru has 2
            ..PLAYER_ATTRIBUTES
        });
        // The enemy opened via the AI and passed the turn. Whatever it
        // chose, it paid stamina for it: the AI never Rests at full, and
        // every other action at full pools costs stamina.
        assert_eq!(turn(&app).side, CombatSide::Player);
        assert!(
            enemy_pools(&mut app).1 < 40,
            "the enemy paid stamina for its opening action"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn key_one_quick_strikes_and_the_drained_enemy_rests() {
        let mut app = test_app();
        press_vs_resting_enemy(&mut app, KeyCode::Digit1);
        // Player: hit for base damage 6, -5 stamina. Enemy reply: forced
        // Rest, +20 stamina. Turn is back with the player.
        assert_eq!(enemy_pools(&mut app), (64, 20));
        assert_eq!(player_pools(&mut app), (90, 45));
        assert_eq!(turn(&app).side, CombatSide::Player);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enemy_reply_waits_for_the_player_presentation_window() {
        let mut app = test_app();
        drain_enemy_stamina(&mut app);
        press(&mut app, KeyCode::Digit1);
        assert_eq!(enemy_pools(&mut app), (64, 0), "player hit resolved");
        assert_eq!(player_pools(&mut app), (90, 45));
        assert_eq!(turn(&app).side, CombatSide::Enemy);
        assert!(presentation_busy(&app));

        app.update();
        assert_eq!(
            enemy_pools(&mut app),
            (64, 0),
            "zero-length frames do not release the reply"
        );
        assert_eq!(
            clip::<PlayerFighter>(&mut app),
            FighterClip::Attack,
            "the player attack is not overwritten by a same-frame reply"
        );

        advance_presentation(&mut app);
        assert_eq!(enemy_pools(&mut app), (64, 20), "enemy rested after delay");
        assert_eq!(turn(&app).side, CombatSide::Player);
        assert!(
            presentation_busy(&app),
            "enemy action now owns presentation"
        );
    }

    #[test]
    fn queued_player_actions_during_presentation_are_discarded() {
        let mut app = test_app();
        drain_enemy_stamina(&mut app);
        app.world_mut()
            .write_message(PlayerActionEvent(CombatAction::QuickStrike));
        app.update();
        assert_eq!(turn(&app).side, CombatSide::Enemy);
        assert!(presentation_busy(&app));
        let player_after_first_action = player_pools(&mut app);

        app.world_mut()
            .write_message(PlayerActionEvent(CombatAction::HeavyStrike));
        app.update();
        advance_presentation(&mut app);
        advance_presentation(&mut app);

        assert_eq!(
            player_pools(&mut app),
            player_after_first_action,
            "queued input during presentation never replays on the next player turn"
        );
        assert_eq!(turn(&app).side, CombatSide::Player);
    }

    /// Equips `id` on the fighter marked `M`; fighters spawn with an empty
    /// [`Equipment`], so this mirrors what the shop issue will do.
    #[cfg(debug_assertions)]
    fn equip<M: Component>(app: &mut App, id: crate::items::ItemId) {
        let mut query = app.world_mut().query_filtered::<&mut Equipment, With<M>>();
        query
            .single_mut(app.world_mut())
            .expect("fighter exists")
            .equip(id);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn an_equipped_weapon_raises_the_damage_written_back() {
        use crate::items::ItemId;
        let mut app = test_app();
        equip::<PlayerFighter>(&mut app, ItemId::Palos); // damage 10
        press_vs_resting_enemy(&mut app, KeyCode::Digit1);
        assert_eq!(
            enemy_pools(&mut app).0,
            54,
            "70 hp - (base damage 6 + Paloș 10)"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn equipped_armor_reduces_the_damage_written_back() {
        use crate::items::ItemId;
        let mut app = test_app();
        equip::<EnemyFighter>(&mut app, ItemId::CojocGros); // armor 2
        equip::<EnemyFighter>(&mut app, ItemId::CaciulaDeOaie); // armor 1
        press_vs_resting_enemy(&mut app, KeyCode::Digit1);
        assert_eq!(
            enemy_pools(&mut app).0,
            67,
            "70 hp - (base damage 6 - armor 3)"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn key_two_heavy_strikes_for_double_damage() {
        let mut app = test_app();
        press_vs_resting_enemy(&mut app, KeyCode::Digit2);
        assert_eq!(enemy_pools(&mut app), (58, 20), "70 hp - 2 * 6 damage");
        assert_eq!(player_pools(&mut app), (90, 35), "heavy strike costs 15");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn movement_actions_update_distance_and_pass_the_turn() {
        let mut app = test_app();
        press_vs_resting_enemy(&mut app, KeyCode::Digit6);
        assert_eq!(turn(&app).distance, DuelDistance::NEAR);
        assert_eq!(
            turn(&app).side,
            CombatSide::Player,
            "the drained enemy rested and passed back"
        );

        press_vs_resting_enemy(&mut app, KeyCode::Digit5);
        assert_eq!(turn(&app).distance, DuelDistance::CLOSE);
        assert_eq!(enemy_pools(&mut app).0, 70, "movement deals no damage");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enemy_advances_back_after_the_player_opens_distance() {
        let mut app = test_app();
        press(&mut app, KeyCode::Digit6);
        assert_eq!(turn(&app).distance, DuelDistance::NEAR);
        assert_eq!(turn(&app).side, CombatSide::Enemy);

        advance_presentation(&mut app);
        assert_eq!(
            turn(&app).distance,
            DuelDistance::CLOSE,
            "the player steps back to near, then the ready enemy steps forward"
        );
        assert_eq!(turn(&app).side, CombatSide::Player);
        assert_eq!(player_pools(&mut app), (90, 50));
        assert_eq!(enemy_pools(&mut app), (70, 40));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn key_three_blocks_and_the_guard_holds_until_the_next_turn() {
        let mut app = test_app();
        press_vs_resting_enemy(&mut app, KeyCode::Digit3);
        assert_eq!(player_pools(&mut app), (90, 47), "block costs 3");
        assert!(turn(&app).player_blocking);
        press_vs_resting_enemy(&mut app, KeyCode::Digit4);
        assert!(!turn(&app).player_blocking, "guard lapses on the next turn");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn key_four_rests_stamina_back_up_to_the_cap() {
        let mut app = test_app();
        for _ in 0..3 {
            press_vs_resting_enemy(&mut app, KeyCode::Digit2); // 3 heavy strikes: 50 -> 5
        }
        assert_eq!(player_pools(&mut app).1, 5);
        press_vs_resting_enemy(&mut app, KeyCode::Digit4);
        assert_eq!(player_pools(&mut app).1, 25, "rest restores 20");
        press_vs_resting_enemy(&mut app, KeyCode::Digit4);
        press_vs_resting_enemy(&mut app, KeyCode::Digit4);
        assert_eq!(player_pools(&mut app).1, 50, "capped at max stamina");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn a_strike_without_stamina_is_a_no_op_but_passes_the_turn() {
        let mut app = test_app();
        for _ in 0..3 {
            press_vs_resting_enemy(&mut app, KeyCode::Digit2); // 50 -> 5 stamina
        }
        assert_eq!(player_pools(&mut app).1, 5);
        let enemy_hp = enemy_pools(&mut app).0;
        press_vs_resting_enemy(&mut app, KeyCode::Digit2); // needs 15, has 5
        assert_eq!(enemy_pools(&mut app).0, enemy_hp, "no damage dealt");
        assert_eq!(player_pools(&mut app).1, 5, "no stamina spent");
        assert_eq!(
            enemy_pools(&mut app).1,
            20,
            "the turn still passed: the drained enemy rested its reply"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn defeat_ends_the_duel_and_stops_combat_input() {
        let mut app = test_app();
        // Quick-strike the enemy down (resting when out of stamina) while
        // keeping the enemy drained so it can only Rest and never fights
        // back; the player can never lose this race.
        for _ in 0..200 {
            if turn(&app).over {
                break;
            }
            drain_enemy_stamina(&mut app);
            let key = if player_pools(&mut app).1 >= 5 {
                KeyCode::Digit1
            } else {
                KeyCode::Digit4
            };
            press(&mut app, key);
            advance_presentation(&mut app);
        }
        assert!(turn(&app).over, "duel ends");
        assert_eq!(enemy_pools(&mut app).0, 0, "enemy is defeated");
        assert!(player_pools(&mut app).0 > 0, "player survives");

        // Input is dead now: nothing changes anymore.
        let before = (player_pools(&mut app), enemy_pools(&mut app));
        press(&mut app, KeyCode::Digit1);
        assert_eq!((player_pools(&mut app), enemy_pools(&mut app)), before);
        assert!(turn(&app).over);
    }

    #[test]
    fn leaving_the_fight_drops_the_combat_resources() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update();
        assert!(app.world().get_resource::<CombatTurn>().is_none());
        assert!(app.world().get_resource::<CombatRng>().is_none());
    }

    /// #200's critical invariant, proven end-to-end through the real ECS
    /// schedule (not just the pure `combat::engine`/`combat::ai` functions
    /// `review::gold_journey_seed` pins): with the exact same `CombatRng`
    /// seed and the exact same sequence of player actions, reduced motion
    /// changes only `arena::fx`/`arena::animation`'s *presentation*
    /// (parallax hold, no camera shake, shrunk lunge/footwork) and never
    /// the duel itself -- costs, outcomes, RNG-driven rolls (crit/hit/AI
    /// choice), and the `CombatPresentation` gating duration are all
    /// bit-for-bit identical. `advance_presentation` always waits out the
    /// full `PRESENTATION_DELAY_SECONDS` window in both runs, so a
    /// regression that made presentation gating itself motion-dependent
    /// would desync the two apps' turn order and fail this test too.
    #[test]
    fn seeded_combat_is_bit_for_bit_identical_with_and_without_reduced_motion() {
        let actions = [
            CombatAction::QuickStrike,
            CombatAction::QuickStrike,
            CombatAction::HeavyStrike,
            CombatAction::Block,
            CombatAction::QuickStrike,
        ];

        let mut full_motion = test_app_with_motion(PLAYER_ATTRIBUTES, false);
        let mut reduced_motion = test_app_with_motion(PLAYER_ATTRIBUTES, true);

        for &action in &actions {
            for app in [&mut full_motion, &mut reduced_motion] {
                if turn(app).over {
                    continue;
                }
                app.world_mut().write_message(PlayerActionEvent(action));
                app.update();
                advance_presentation(app);
                advance_presentation(app);
            }
        }

        assert_eq!(
            player_pools(&mut full_motion),
            player_pools(&mut reduced_motion),
            "player health/stamina match regardless of the motion preference"
        );
        assert_eq!(
            enemy_pools(&mut full_motion),
            enemy_pools(&mut reduced_motion),
            "enemy health/stamina (and thus every RNG-driven roll) match"
        );
        assert_eq!(
            turn(&full_motion),
            turn(&reduced_motion),
            "turn side, block flags, over flag, and duel distance all match"
        );
    }
}
