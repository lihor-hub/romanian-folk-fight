//! Progression: the run currency (galbeni), the outcome of a finished fight,
//! and the flow off the fight screen — victory pays out on the result screen,
//! defeat leads to game over and a run reset.
//!
//! The plugin watches the same [`CombatLogEvent`] stream as the HUD log and
//! the announcer; on [`CombatEvent::Defeated`] it records a [`FightOutcome`]
//! and, after a short delay so the final blow stays visible, emits a
//! [`FlowIntent`] (`ResolveVictory`, `ResolveDefeat`, or `RunWon`) for
//! [`crate::flow::apply_flow_intents`] — the sole runtime writer of
//! `NextState<GameState>` (#166) — to route to [`GameState::FightResult`],
//! [`GameState::GameOver`], or [`GameState::Victory`].

pub mod level;
pub mod result_ui;
pub mod victory_ui;

pub use level::{Level, LevelUpDraft, POINTS_PER_LEVEL, top_up_pool, xp_to_next};

use bevy::prelude::*;

use crate::combat::{CombatEvent, CombatLogEvent, CombatSide};
use crate::core::{GameState, despawn_screen};
use crate::creation::PlayerCharacter;
use crate::flow::FlowIntent;
use crate::roster::LadderProgress;
use crate::save::SaveRequested;
use crate::shop::{OwnedItems, PlayerEquipment};
use crate::ui_widgets::focus::{FocusNavigationPlugin, FocusNavigationSet};

/// Galbeni a fresh run starts with, so the first shop visit isn't pointless.
pub const STARTING_GALBENI: u32 = 50;
/// Flat part of the victory payout.
pub const REWARD_BASE: u32 = 25;
/// Per-enemy-level part of the victory payout.
pub const REWARD_PER_LEVEL: u32 = 10;
/// XP payout per enemy level for a victory.
pub const XP_PER_LEVEL: u32 = 20;
/// Seconds between the killing blow and leaving the fight screen, so the
/// final hit (and its announcer line) stays visible.
pub const FIGHT_END_DELAY_SECONDS: f32 = 1.5;

/// Victory payout in galbeni for beating an enemy of `enemy_level`.
pub fn fight_reward(enemy_level: u32) -> u32 {
    REWARD_BASE + REWARD_PER_LEVEL * enemy_level
}

/// XP for beating an enemy of `enemy_level`: `20 * enemy_level`, doubled for
/// bosses (level and flag come from the opponent ladder).
pub fn fight_xp(enemy_level: u32, is_boss: bool) -> u32 {
    let base = XP_PER_LEVEL * enemy_level;
    if is_boss { 2 * base } else { base }
}

/// The player's run currency, in galbeni. Reset to [`STARTING_GALBENI`] when
/// a run ends (see [`reset_run`]).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wallet(pub u32);

impl Default for Wallet {
    fn default() -> Self {
        Self(STARTING_GALBENI)
    }
}

/// Total galbeni earned over the run — every victory payout, regardless of
/// what was later spent in the shop. Shown in the victory recap; reset with
/// the run.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LifetimeEarnings(pub u32);

/// Fired once when the lap-1 final boss falls and the run is won. The
/// victory sting (#24) is not wired here to keep the audio module decoupled:
/// the audio plugin, if present, observes this message and plays the sting.
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct VictoryEvent;

/// How the last fight ended. Written once per fight when `Defeated` fires,
/// cleared when the next fight starts, and read by the result and game-over
/// screens.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FightOutcome {
    /// The side left standing.
    pub winner: CombatSide,
    /// The defeated side.
    pub loser: CombatSide,
    /// Victory payout in galbeni; only credited when the player won.
    pub reward: u32,
    /// XP payout; stored but unused until the leveling issue.
    pub xp: u32,
    /// Whether [`Wallet`] was already credited for this outcome — the guard
    /// that makes re-entering the result screen award nothing twice.
    pub rewarded: bool,
}

impl FightOutcome {
    /// The outcome recorded when `winner` lands the killing blow on an enemy
    /// of `enemy_level`; bosses (`is_boss`) pay double XP.
    pub fn from_defeat(winner: CombatSide, enemy_level: u32, is_boss: bool) -> Self {
        Self {
            winner,
            loser: winner.opponent(),
            reward: fight_reward(enemy_level),
            xp: fight_xp(enemy_level, is_boss),
            rewarded: false,
        }
    }
}

/// Countdown between the killing blow and the transition off the fight
/// screen. Inserted next to [`FightOutcome`] and removed when it fires (or
/// when the fight screen exits some other way).
#[derive(Resource, Debug)]
pub struct FightEndDelay(pub Timer);

pub struct ProgressionPlugin;

impl Plugin for ProgressionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wallet>()
            .init_resource::<Level>()
            .init_resource::<LifetimeEarnings>()
            .add_message::<SaveRequested>()
            .add_message::<VictoryEvent>()
            .add_plugins((crate::ui_widgets::ScrollInputPlugin, FocusNavigationPlugin))
            .add_systems(OnEnter(GameState::Fight), clear_fight_outcome)
            .add_systems(
                Update,
                (
                    detect_fight_end,
                    tick_fight_end_delay.in_set(crate::flow::FlowIntentEmission),
                )
                    .chain()
                    .run_if(in_state(GameState::Fight)),
            )
            .add_systems(OnExit(GameState::Fight), clear_fight_end_delay)
            .add_systems(
                OnEnter(GameState::FightResult),
                (award_victory, result_ui::spawn_result_screen).chain(),
            )
            .add_systems(
                Update,
                (
                    result_ui::handle_result_actions
                        .in_set(crate::flow::FlowIntentEmission)
                        .after(FocusNavigationSet),
                    result_ui::handle_allocation_actions.after(FocusNavigationSet),
                    result_ui::update_button_backgrounds,
                    result_ui::update_allocation_labels
                        .run_if(resource_exists_and_changed::<LevelUpDraft>),
                    result_ui::resize_result_screens,
                    // #216: the result screen root scrolls on short
                    // viewports (see `result_ui::screen_root`).
                    crate::ui_widgets::scroll_with_wheel_and_touch,
                )
                    .chain()
                    .run_if(in_state(GameState::FightResult)),
            )
            .add_systems(
                OnExit(GameState::FightResult),
                (
                    despawn_screen::<result_ui::ResultScreen>,
                    clear_level_up_draft,
                ),
            )
            .add_systems(
                OnEnter(GameState::GameOver),
                result_ui::spawn_game_over_screen,
            )
            .add_systems(
                Update,
                (
                    result_ui::handle_game_over_actions
                        .in_set(crate::flow::FlowIntentEmission)
                        .after(FocusNavigationSet),
                    result_ui::update_button_backgrounds,
                    result_ui::resize_result_screens,
                    crate::ui_widgets::scroll_with_wheel_and_touch,
                )
                    .run_if(in_state(GameState::GameOver)),
            )
            .add_systems(
                OnExit(GameState::GameOver),
                despawn_screen::<result_ui::GameOverScreen>,
            )
            .add_systems(
                OnEnter(GameState::Victory),
                (award_victory, victory_ui::spawn_victory_screen).chain(),
            )
            .add_systems(
                Update,
                (
                    victory_ui::handle_victory_actions
                        .in_set(crate::flow::FlowIntentEmission)
                        .after(FocusNavigationSet),
                    result_ui::update_button_backgrounds,
                    crate::ui_widgets::scroll_with_wheel_and_touch,
                )
                    .run_if(in_state(GameState::Victory)),
            )
            .add_systems(
                OnExit(GameState::Victory),
                despawn_screen::<victory_ui::VictoryScreen>,
            );
    }
}

/// Drops the previous fight's outcome so a fresh duel starts clean.
fn clear_fight_outcome(mut commands: Commands) {
    commands.remove_resource::<FightOutcome>();
}

/// Drops the end-of-fight countdown when the fight screen exits, however it
/// exits.
fn clear_fight_end_delay(mut commands: Commands) {
    commands.remove_resource::<FightEndDelay>();
}

/// Drops the level-up allocation draft when the result screen exits: only a
/// confirmed allocation touches [`PlayerCharacter`] and [`Level`], so points
/// left in an abandoned draft simply stay unspent (a fresh draft is built on
/// the next visit).
fn clear_level_up_draft(mut commands: Commands) {
    commands.remove_resource::<LevelUpDraft>();
}

/// Watches the combat log for the killing blow: records the [`FightOutcome`]
/// (the actor of the `Defeated` event is the winner, the reward and XP come
/// from the current ladder opponent's level and boss flag) and arms the
/// end-of-fight delay. Only the first `Defeated` of a fight counts.
fn detect_fight_end(
    mut commands: Commands,
    mut events: MessageReader<CombatLogEvent>,
    outcome: Option<Res<FightOutcome>>,
    ladder: Option<Res<LadderProgress>>,
) {
    let (enemy_level, enemy_is_boss) = ladder.map_or((1, false), |ladder| {
        let opponent = ladder.opponent();
        (opponent.level, opponent.is_boss)
    });
    let mut already_ended = outcome.is_some();
    for event in events.read() {
        if already_ended || event.event != CombatEvent::Defeated {
            continue;
        }
        already_ended = true;
        commands.insert_resource(FightOutcome::from_defeat(
            event.actor,
            enemy_level,
            enemy_is_boss,
        ));
        commands.insert_resource(FightEndDelay(Timer::from_seconds(
            FIGHT_END_DELAY_SECONDS,
            TimerMode::Once,
        )));
    }
}

/// Counts down after the killing blow, then leaves the fight screen: the
/// player's victory goes to the result screen — or, when the fallen opponent
/// was the lap-1 final boss, to the victory ending (#26) — and their defeat
/// to game over. Later laps keep the loop behavior.
///
/// #166: this no longer writes `NextState<GameState>` itself. It emits the
/// matching automated [`FlowIntent`] (`ResolveVictory`/`ResolveDefeat`/
/// `RunWon`) and joins the [`crate::flow::FlowIntentEmission`] system set, so
/// [`crate::flow::apply_flow_intents`] — the sole runtime writer of
/// `NextState<GameState>` — applies the transition later in the same
/// `Update` pass, preserving the exact two-`app.update()` timing the fight-
/// end delay had before (the transition is queued the frame the delay
/// expires, and applied on the following frame's state-transition pass).
fn tick_fight_end_delay(
    mut commands: Commands,
    time: Res<Time>,
    delay: Option<ResMut<FightEndDelay>>,
    outcome: Option<Res<FightOutcome>>,
    ladder: Option<Res<LadderProgress>>,
    mut victories: MessageWriter<VictoryEvent>,
    mut flow_intents: MessageWriter<FlowIntent>,
) {
    let (Some(mut delay), Some(outcome)) = (delay, outcome) else {
        return;
    };
    if !delay.0.tick(time.delta()).is_finished() {
        return;
    }
    commands.remove_resource::<FightEndDelay>();
    let run_won = outcome.winner == CombatSide::Player
        && ladder.is_some_and(|ladder| ladder.is_final_lap_one_fight());
    let intent = match outcome.winner {
        CombatSide::Player if run_won => {
            victories.write(VictoryEvent);
            FlowIntent::RunWon
        }
        CombatSide::Player => FlowIntent::ResolveVictory,
        CombatSide::Enemy => FlowIntent::ResolveDefeat,
    };
    flow_intents.write(intent);
}

/// Credits the victory payout — galbeni to the wallet, XP to [`Level`] with
/// every level-up it affords — and advances the opponent ladder, exactly
/// once per fight; the `rewarded` flag guards against a double award (or
/// double advance) if the result screen is re-entered (e.g. via the shop)
/// before the next fight clears the outcome. The credited run is autosaved
/// (see [`crate::save`]).
fn award_victory(
    mut wallet: ResMut<Wallet>,
    mut level: ResMut<Level>,
    mut earnings: ResMut<LifetimeEarnings>,
    outcome: Option<ResMut<FightOutcome>>,
    ladder: Option<ResMut<LadderProgress>>,
    mut save_requests: MessageWriter<SaveRequested>,
) {
    let Some(mut outcome) = outcome else {
        warn!("entered GameState::FightResult without a FightOutcome; nothing to award");
        return;
    };
    if outcome.rewarded || outcome.winner != CombatSide::Player {
        return;
    }
    wallet.0 += outcome.reward;
    earnings.0 += outcome.reward;
    level.gain_xp(outcome.xp);
    if let Some(mut ladder) = ladder {
        ladder.advance();
    }
    outcome.rewarded = true;
    save_requests.write(SaveRequested);
}

/// Resets every run-scoped resource so the next run starts clean: a fresh
/// [`Wallet`], level 1 with no XP or points, no owned or equipped shop gear,
/// the opponent ladder back at the first rung, no confirmed
/// [`PlayerCharacter`], no stale [`FightOutcome`].
pub(crate) fn reset_run(commands: &mut Commands) {
    commands.insert_resource(Wallet::default());
    commands.insert_resource(Level::default());
    commands.insert_resource(LifetimeEarnings::default());
    commands.insert_resource(LadderProgress::default());
    commands.insert_resource(OwnedItems::default());
    commands.insert_resource(PlayerEquipment::default());
    commands.remove_resource::<PlayerCharacter>();
    commands.remove_resource::<FightOutcome>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CorePlugin;
    use crate::flow::FlowPlugin;
    use bevy::state::app::StatesPlugin;
    use std::time::Duration;

    /// Headless app with the progression flow and the combat-log message
    /// registered (combat itself is not needed to drive the flow).
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

    fn set_state(app: &mut App, state: GameState) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(state);
        app.update();
    }

    fn state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    fn wallet(app: &App) -> u32 {
        app.world().resource::<Wallet>().0
    }

    /// Injects the killing blow: `actor` defeats their opponent.
    fn write_defeat(app: &mut App, actor: CombatSide) {
        app.world_mut().write_message(CombatLogEvent {
            actor,
            action: crate::combat::CombatAction::QuickStrike,
            event: CombatEvent::Defeated,
        });
        app.update();
    }

    /// Force-expires the end-of-fight delay, then runs the transition.
    fn expire_delay(app: &mut App) {
        app.world_mut()
            .resource_mut::<FightEndDelay>()
            .0
            .tick(Duration::from_secs_f32(FIGHT_END_DELAY_SECONDS + 1.0));
        app.update(); // tick system queues the transition
        app.update(); // transition applies, OnEnter runs
    }

    #[test]
    fn the_reward_scales_with_the_enemy_level() {
        assert_eq!(fight_reward(1), 35, "25 + 10 * 1");
        assert_eq!(fight_reward(2), 45);
        assert_eq!(fight_reward(5), 75);
    }

    #[test]
    fn the_xp_scales_with_the_enemy_level_and_doubles_for_bosses() {
        assert_eq!(fight_xp(1, false), 20, "20 * 1");
        assert_eq!(fight_xp(3, false), 60);
        assert_eq!(fight_xp(1, true), 40, "bosses pay double");
        assert_eq!(fight_xp(3, true), 120);
    }

    #[test]
    fn a_fresh_wallet_holds_fifty_galbeni() {
        let app = test_app();
        assert_eq!(wallet(&app), STARTING_GALBENI);
        assert_eq!(Wallet::default(), Wallet(50));
    }

    #[test]
    fn a_player_kill_records_the_outcome_and_waits_before_the_result_screen() {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);

        assert_eq!(state(&app), GameState::Fight, "the final blow lingers");
        let outcome = *app.world().resource::<FightOutcome>();
        assert_eq!(
            outcome,
            FightOutcome {
                winner: CombatSide::Player,
                loser: CombatSide::Enemy,
                reward: 35,
                xp: fight_xp(1, false),
                rewarded: false,
            },
            "without a ladder the outcome falls back to a level-1 non-boss"
        );
        app.update();
        assert_eq!(state(&app), GameState::Fight, "still waiting out the delay");

        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::FightResult);
        assert!(
            app.world().get_resource::<FightEndDelay>().is_none(),
            "the countdown is dropped once it fires"
        );
    }

    #[test]
    fn a_player_defeat_leads_to_game_over_without_pay() {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Enemy);
        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::GameOver);
        assert_eq!(wallet(&app), 50, "losing pays nothing");
    }

    #[test]
    fn only_the_first_defeated_event_of_a_fight_counts() {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);
        write_defeat(&mut app, CombatSide::Enemy);
        assert_eq!(
            app.world().resource::<FightOutcome>().winner,
            CombatSide::Player,
            "the first killing blow decides the fight"
        );
        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::FightResult);
    }

    #[test]
    fn victory_credits_the_wallet_exactly_once() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);
        assert_eq!(wallet(&app), 85, "50 + reward 35, credited on entry");
        assert!(app.world().resource::<FightOutcome>().rewarded);

        // A detour to the shop and back must not pay again.
        set_state(&mut app, GameState::Shop);
        set_state(&mut app, GameState::FightResult);
        assert_eq!(wallet(&app), 85, "re-entry never double-awards");
    }

    #[test]
    fn an_enemy_victory_never_credits_the_wallet() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Enemy, 1, false));
        set_state(&mut app, GameState::FightResult);
        assert_eq!(wallet(&app), 50, "only the player's wins pay out");
        assert_eq!(
            *app.world().resource::<Level>(),
            Level::default(),
            "only the player's wins grant XP"
        );
    }

    #[test]
    fn victory_grants_xp_exactly_once() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);
        assert_eq!(
            *app.world().resource::<Level>(),
            Level {
                level: 1,
                xp: 20,
                unspent_points: 0,
            },
            "the level-1 non-boss enemy pays 20 XP on entry"
        );

        // A detour to the shop and back must not grant again.
        set_state(&mut app, GameState::Shop);
        set_state(&mut app, GameState::FightResult);
        assert_eq!(app.world().resource::<Level>().xp, 20);
    }

    #[test]
    fn an_award_over_the_threshold_levels_up_with_carry() {
        let mut app = test_app();
        app.insert_resource(Level {
            level: 1,
            xp: 90,
            unspent_points: 0,
        });
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);
        assert_eq!(
            *app.world().resource::<Level>(),
            Level {
                level: 2,
                xp: 10,
                unspent_points: POINTS_PER_LEVEL,
            },
            "90 + 20 crosses the 100 XP threshold and carries 10"
        );
    }

    #[test]
    fn entering_a_new_fight_clears_the_previous_outcome() {
        let mut app = test_app();
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::Fight);
        assert!(
            app.world().get_resource::<FightOutcome>().is_none(),
            "a fresh fight has no outcome yet"
        );
    }

    #[test]
    fn the_outcome_takes_level_and_boss_flag_from_the_ladder() {
        let mut app = test_app();
        // LadderProgress(4) is the first boss fight: Muma Pădurii, level 5.
        app.insert_resource(LadderProgress(4));
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);

        let outcome = *app.world().resource::<FightOutcome>();
        assert_eq!(outcome.reward, fight_reward(5), "level-5 payout");
        assert_eq!(outcome.xp, fight_xp(5, true), "boss XP is doubled");
        assert_eq!(outcome.xp, 200, "20 * 5 * 2");
    }

    #[test]
    fn victory_advances_the_ladder_exactly_once() {
        let mut app = test_app();
        app.insert_resource(LadderProgress(3));
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 4, false));
        set_state(&mut app, GameState::FightResult);
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress(4),
            "the win moves the run to the next opponent"
        );

        // A detour to the shop and back must not advance again.
        set_state(&mut app, GameState::Shop);
        set_state(&mut app, GameState::FightResult);
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress(4),
            "re-entry never double-advances"
        );
    }

    #[test]
    fn a_defeat_never_advances_the_ladder() {
        let mut app = test_app();
        app.insert_resource(LadderProgress(3));
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Enemy);
        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::GameOver);
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress(3),
            "losing keeps the run on the same opponent"
        );
    }

    #[test]
    fn beating_the_last_opponent_on_lap_one_routes_to_victory() {
        let mut app = test_app();
        app.insert_resource(LadderProgress(9));
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);
        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::Victory);
        assert_eq!(
            *app.world().resource::<LadderProgress>(),
            LadderProgress(10),
            "the win still advances the ladder into lap 2"
        );
        assert_eq!(
            wallet(&app),
            50 + fight_reward(10),
            "the final payout is credited on the victory screen"
        );
    }

    #[test]
    fn the_run_win_emits_the_victory_event_for_the_audio_hook() {
        let mut app = test_app();
        app.insert_resource(LadderProgress(9));
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);

        let mut cursor = app
            .world()
            .resource::<Messages<VictoryEvent>>()
            .get_cursor();
        assert_eq!(
            cursor
                .read(app.world().resource::<Messages<VictoryEvent>>())
                .count(),
            0,
            "no victory before the delay expires"
        );
        expire_delay(&mut app);
        assert_eq!(
            cursor
                .read(app.world().resource::<Messages<VictoryEvent>>())
                .count(),
            1,
            "exactly one VictoryEvent fires with the transition"
        );
    }

    #[test]
    fn the_lap_two_final_boss_keeps_the_loop_behavior() {
        let mut app = test_app();
        app.insert_resource(LadderProgress(19));
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);
        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::FightResult);
    }

    #[test]
    fn a_non_final_win_routes_to_the_result_screen() {
        let mut app = test_app();
        app.insert_resource(LadderProgress(4));
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);
        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::FightResult);
    }

    #[test]
    fn losing_the_final_fight_still_leads_to_game_over() {
        let mut app = test_app();
        app.insert_resource(LadderProgress(9));
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Enemy);
        expire_delay(&mut app);
        assert_eq!(state(&app), GameState::GameOver);
    }

    #[test]
    fn lifetime_earnings_accumulate_across_wins_and_reset_with_the_run() {
        let mut app = test_app();
        assert_eq!(
            *app.world().resource::<LifetimeEarnings>(),
            LifetimeEarnings(0),
            "a fresh run has earned nothing yet"
        );

        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 1, false));
        set_state(&mut app, GameState::FightResult);
        set_state(&mut app, GameState::Fight);
        app.insert_resource(FightOutcome::from_defeat(CombatSide::Player, 2, false));
        set_state(&mut app, GameState::FightResult);
        assert_eq!(
            *app.world().resource::<LifetimeEarnings>(),
            LifetimeEarnings(fight_reward(1) + fight_reward(2)),
            "every payout adds up, regardless of spending"
        );

        // Spending never touches the lifetime total.
        app.world_mut().resource_mut::<Wallet>().0 = 0;
        assert_eq!(
            *app.world().resource::<LifetimeEarnings>(),
            LifetimeEarnings(fight_reward(1) + fight_reward(2)),
        );

        set_state(&mut app, GameState::GameOver);
        press_game_over_back_to_menu(&mut app);
        assert_eq!(
            *app.world().resource::<LifetimeEarnings>(),
            LifetimeEarnings(0),
            "the run reset clears the lifetime total"
        );
    }

    /// Presses the game-over screen's back-to-menu button.
    fn press_game_over_back_to_menu(app: &mut App) {
        let button = app
            .world_mut()
            .query_filtered::<Entity, (With<Button>, With<result_ui::GameOverAction>)>()
            .single(app.world())
            .expect("back-to-menu button exists");
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
    }

    #[test]
    fn leaving_the_fight_early_drops_the_end_delay() {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        write_defeat(&mut app, CombatSide::Player);
        assert!(app.world().get_resource::<FightEndDelay>().is_some());
        set_state(&mut app, GameState::MainMenu);
        assert!(
            app.world().get_resource::<FightEndDelay>().is_none(),
            "OnExit(Fight) clears the countdown"
        );
    }
}
