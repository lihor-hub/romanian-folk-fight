//! Flow plugin (#155, #164, #166): the single validated table of
//! [`GameState`] transitions — menu/creation navigation, the
//! player-triggered routes out of a fight (result, shop, victory, game-over,
//! paused-fight abandon), and the automated combat-outcome routes (delayed
//! win/defeat/run-completion). Screens and gameplay systems keep their domain
//! side effects (run reset, hero/loadout creation, save restore, rewards,
//! purchases, the fight-end delay itself) but emit a [`FlowIntent`] for
//! navigation instead of writing `NextState<GameState>` directly.
//! [`apply_flow_intents`] is the **only** system, anywhere in runtime code,
//! that writes `NextState<GameState>` — see
//! `flow_plugin_is_the_sole_runtime_next_state_game_state_writer` in this
//! module's tests for the regression check that enforces it (and its one
//! documented, pre-existing exception).
//!
//! # Transition table
//!
//! Every row below is a `(current state, intent) -> next state` route; any
//! pair not listed is rejected (see [`TransitionResult::Rejected`]). Rows are
//! grouped by which slice introduced them; [`transition_for`] is the single
//! source of truth and `table_covers_exactly_the_owned_rows` (in this
//! module's tests) asserts nothing else sneaks in.
//!
//! | Current state | Intent | Next state | Owner | Trigger |
//! |---|---|---|---|---|
//! | `MainMenu` | `StartNewGame` | `CharacterCreation` | #155 | button |
//! | `MainMenu` | `ContinueRun` | `Fight` | #155 | button |
//! | `MainMenu` | `ContinueToShop` | `Shop` | #217 | button |
//! | `MainMenu` | `ContinueToTown` | `Town` | #129 | button |
//! | `CharacterCreation` | `ConfirmHero` | `Town` | #155, retargeted by #129 | button |
//! | `CharacterCreation` | `BackToMenu` | `MainMenu` | #155 | button |
//! | `Town` | `EnterArena` | `Fight` | #129 | button |
//! | `Town` | `GoToShop` | `Shop` | #129 | button |
//! | `Town` | `BackToMenu` | `MainMenu` | #129 | button |
//! | `FightResult` | `GoToTown` | `Town` | #129 | button |
//! | `Shop` | `GoToTown` | `Town` | #129 | button |
//! | `Victory` | `NextLap` | `Town` | #164, retargeted by #129 | button |
//! | `Victory` | `BackToMenu` | `MainMenu` | #164 | button |
//! | `GameOver` | `BackToMenu` | `MainMenu` | #164 | button |
//! | `Fight` | `AbandonFight` | `MainMenu` | #164 | button (paused) |
//! | `Fight` | `ResolveVictory` | `FightResult` | #166 | automated (fight-end delay) |
//! | `Fight` | `ResolveDefeat` | `GameOver` | #166 | automated (fight-end delay) |
//! | `Fight` | `RunWon` | `Victory` | #166 | automated (fight-end delay) |
//!
//! #129 (the Town hub, `docs/navigation-proposal.md`) reshaped the loop from
//! `Fight → FightResult → Shop → Fight` into
//! `Town → Fight → FightResult → Town`: the shop became an optional
//! detour off the hub, so the old `FightResult → Shop` (`GoToShop`),
//! `FightResult → Fight` (`NextFight`), and `Shop → Fight` (`BackToArena`)
//! rows are gone — every post-fight and post-shop route funnels back
//! through `Town`.
//!
//! # Extending the table (procedure for #146 and future campaign issues)
//!
//! 1. Add the new route(s) to [`FlowIntent`] (a new variant, or reuse an
//!    existing one if the destination and semantics truly match — see the
//!    `BackToMenu` doc comment for when reuse is appropriate vs. not).
//! 2. Add the corresponding arm(s) to [`transition_for`]. This function is
//!    the only place a row may be added — do not special-case anything in
//!    [`apply_flow_intents`] itself, which stays generic over the table.
//! 3. Add the new row(s) to the table above, and to `owned`/`all_states`/
//!    `all_intents` in `table_covers_exactly_the_owned_rows` so the
//!    "nothing stray" assertion covers them.
//! 4. Add a row-level test (`transition_for(from, intent) == Some(to)`) next
//!    to the existing ones.
//! 5. At the emitting side (a screen's button handler, or an automated
//!    system like `tick_fight_end_delay`): perform the domain side effect
//!    first (reward, save, reset — whatever the destination implies), *then*
//!    write the [`FlowIntent`], and add that system to
//!    [`FlowIntentEmission`]. Never write `NextState<GameState>` directly —
//!    the sole-owner regression test in this module will fail the build if
//!    a new writer appears outside `src/flow/`.
//! 6. If the new route needs a full journey test (not just a table row),
//!    extend the journey harness in this module's tests (see
//!    `menu_to_shop_journey_reaches_the_next_fight` and its siblings) rather
//!    than adding a one-off integration test elsewhere.

use bevy::prelude::*;

use crate::core::GameState;

/// A navigation request emitted by a screen. Applying it is the only thing
/// that changes `GameState` for the routes owned by this slice.
///
/// **Ordering contract**: the emitting system must have already applied its
/// domain side effect (run reset, hero/loadout creation, save restore)
/// *before* writing the intent — [`apply_flow_intents`] only routes state,
/// it never performs domain work.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowIntent {
    /// Menu → creation: start a fresh run. The run reset must already have
    /// happened.
    StartNewGame,
    /// Menu → Fight: resume a restored save whose stored
    /// [`crate::save::ResumeDestination`] is `Fight` (hero confirmation and
    /// the result/reward checkpoint both resume here). The save must already
    /// be restored into the run resources.
    ContinueRun,
    /// Menu → Shop: resume a restored save whose stored
    /// [`crate::save::ResumeDestination`] is `Shop` (the shop-entry and
    /// purchase/equip checkpoints resume here) (#217). The save must already
    /// be restored into the run resources.
    ContinueToShop,
    /// Menu → Town: resume a restored save whose stored
    /// [`crate::save::ResumeDestination`] is `Town` — the default
    /// destination every non-shop checkpoint writes since #129. The save
    /// must already be restored into the run resources.
    ContinueToTown,
    /// Creation → Town: the hero/loadout is confirmed and stored; the run
    /// starts at the hub (#129).
    ConfirmHero,
    /// Creation → menu, game-over → menu (with the run reset), victory →
    /// menu (with the looping run's save kept), or town → menu (with the
    /// save kept, #129): every "back to the main menu" button shares this
    /// intent since the table only routes state — the emitting screen
    /// already applied whatever domain effect (or none) its own destination
    /// implies.
    BackToMenu,
    /// Town → Shop: visit the shop (**Prăvălie**, #129). No domain side
    /// effect — the shop autosaves on entry.
    GoToShop,
    /// Town → Fight: start the next ladder fight (**Luptă în arenă**, the
    /// hub's dominant primary action, #129).
    EnterArena,
    /// FightResult → Town (**Continuă**; the reward was already credited on
    /// `OnEnter(FightResult)`) and Shop → Town (**Înapoi**; purchases/equips
    /// already applied as they were pressed): every "back to the hub" button
    /// shares this intent, mirroring [`FlowIntent::BackToMenu`]'s reuse
    /// rationale (#129).
    GoToTown,
    /// Victory → Town: continue the looping run into lap 2 (**Turul 2**,
    /// retargeted from the shop by #129). The ladder already advanced past
    /// the last lap-1 opponent.
    NextLap,
    /// Paused Fight → menu: **Abandonează**. Not a defeat, but a forfeit
    /// (#217): the run snapshot is already cleared and every run-scoped
    /// resource already reset before this is emitted (see
    /// `combat::pause::handle_overlay_buttons`), so **Continuă** goes back to
    /// disabled and there is no fresh full-health retry of the abandoned
    /// fight -- only a whole new run via character creation.
    AbandonFight,
    /// Fight → FightResult: the player won a non-final fight. Emitted by
    /// `progression::tick_fight_end_delay` once the end-of-fight delay
    /// expires; the reward is credited afterward, on `OnEnter(FightResult)`.
    ResolveVictory,
    /// Fight → GameOver: the player lost. Emitted by
    /// `progression::tick_fight_end_delay` once the end-of-fight delay
    /// expires. No reward, no ladder advance.
    ResolveDefeat,
    /// Fight → Victory: the player won the lap-1 final boss fight and the
    /// run is complete. Emitted by `progression::tick_fight_end_delay` once
    /// the end-of-fight delay expires, alongside the [`crate::progression::VictoryEvent`]
    /// message (for the audio sting).
    RunWon,
}

/// Outcome of applying one [`FlowIntent`] against the transition table.
/// Reported (as a message, and via `warn!` for rejections) so invalid and
/// duplicate intents have a deterministic, observable result instead of a
/// silent no-op.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionResult {
    /// The table had a row for `(from, intent)`; `NextState` now targets `to`.
    Applied {
        intent: FlowIntent,
        from: GameState,
        to: GameState,
    },
    /// No row for `(from, intent)` — either the intent does not apply to the
    /// current state, or it is a duplicate that already got consumed earlier
    /// in the same frame (the *effective* current state moved on). State is
    /// left unchanged.
    Rejected { intent: FlowIntent, from: GameState },
}

/// The transition table: exactly the routes owned by menu/creation (#155),
/// post-fight/pause navigation (#164), and automated combat outcomes
/// (#166). `None` covers every other `(state, intent)` pair. See the module
/// docs above for the human-readable table and the extension procedure.
fn transition_for(from: GameState, intent: FlowIntent) -> Option<GameState> {
    use FlowIntent::*;
    use GameState::*;
    match (from, intent) {
        (MainMenu, StartNewGame) => Some(CharacterCreation),
        (MainMenu, ContinueRun) => Some(Fight),
        (MainMenu, ContinueToShop) => Some(Shop),
        (MainMenu, ContinueToTown) => Some(Town),
        (CharacterCreation, ConfirmHero) => Some(Town),
        (CharacterCreation, BackToMenu) => Some(MainMenu),
        (Town, EnterArena) => Some(Fight),
        (Town, GoToShop) => Some(Shop),
        (Town, BackToMenu) => Some(MainMenu),
        (FightResult, GoToTown) => Some(Town),
        (Shop, GoToTown) => Some(Town),
        (Victory, NextLap) => Some(Town),
        (Victory, BackToMenu) => Some(MainMenu),
        (GameOver, BackToMenu) => Some(MainMenu),
        (Fight, AbandonFight) => Some(MainMenu),
        (Fight, ResolveVictory) => Some(FightResult),
        (Fight, ResolveDefeat) => Some(GameOver),
        (Fight, RunWon) => Some(Victory),
        _ => None,
    }
}

/// System set covering everything that may emit a [`FlowIntent`] this frame.
/// Screens add their intent-emitting systems to this set; [`FlowPlugin`]
/// orders [`apply_flow_intents`] after it so same-frame intents are always
/// seen (matching the timing screens previously got from writing
/// `NextState` directly: the transition is queued the same frame the button
/// is pressed, and applied on the following frame's state-transition pass).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowIntentEmission;

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FlowIntent>()
            .add_message::<TransitionResult>()
            .add_systems(Update, apply_flow_intents.after(FlowIntentEmission));
    }
}

/// The sole writer of `NextState<GameState>` for menu/creation navigation.
///
/// Intents are applied in emission order, tracking an *effective* current
/// state that starts at the real current state and advances with each
/// applied intent. This makes a duplicate (or now-stale) intent queued in
/// the same frame a deterministic rejection rather than a second call to
/// `NextState::set` silently clobbering the first: the table is checked
/// against where the run would *now* be, not the frame's original state.
fn apply_flow_intents(
    mut intents: MessageReader<FlowIntent>,
    mut results: MessageWriter<TransitionResult>,
    state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let mut effective = *state.get();
    for intent in intents.read() {
        match transition_for(effective, *intent) {
            Some(to) => {
                next_state.set(to);
                results.write(TransitionResult::Applied {
                    intent: *intent,
                    from: effective,
                    to,
                });
                effective = to;
            }
            None => {
                warn!(
                    ?intent,
                    ?effective,
                    "flow intent rejected: no transition row for the current state"
                );
                results.write(TransitionResult::Rejected {
                    intent: *intent,
                    from: effective,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, FlowPlugin));
        app.init_state::<GameState>();
        app.update();
        // The production initial state is `GameState::Loading` (#114), whose
        // fall-through lives in `CorePlugin` — not added here. This suite
        // exercises menu/creation routing, so start where those routes begin.
        set_state(&mut app, GameState::MainMenu);
        app
    }

    fn set_state(app: &mut App, state: GameState) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(state);
        app.update();
    }

    fn write_intent(app: &mut App, intent: FlowIntent) {
        app.world_mut()
            .resource_mut::<Messages<FlowIntent>>()
            .write(intent);
    }

    fn results(app: &mut App) -> Vec<TransitionResult> {
        let messages = app.world().resource::<Messages<TransitionResult>>();
        messages.get_cursor().read(messages).copied().collect()
    }

    fn current_state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    /// Journey-harness primitive: writes `intent`, lets it queue (one
    /// `update`) and apply (a second `update`, matching the two-update
    /// contract in `a_valid_intent_queues_next_state_the_same_frame_it_is_applied`),
    /// and returns the resulting state. Chaining calls drives a full
    /// multi-step journey through the transition table.
    fn step(app: &mut App, intent: FlowIntent) -> GameState {
        write_intent(app, intent);
        app.update();
        app.update();
        current_state(app)
    }

    /// `NextState` has no `PartialEq` impl in this Bevy version, so callers
    /// compare against an expected variant with `matches!` instead.
    fn next_state_is_pending(app: &App, expected: GameState) -> bool {
        matches!(
            *app.world().resource::<NextState<GameState>>(),
            NextState::Pending(state) if state == expected
        )
    }

    fn next_state_is_unchanged(app: &App) -> bool {
        matches!(
            *app.world().resource::<NextState<GameState>>(),
            NextState::Unchanged
        )
    }

    // --- Transition table rows ---

    #[test]
    fn menu_start_new_game_routes_to_creation() {
        assert_eq!(
            transition_for(GameState::MainMenu, FlowIntent::StartNewGame),
            Some(GameState::CharacterCreation)
        );
    }

    #[test]
    fn menu_continue_run_routes_to_fight() {
        assert_eq!(
            transition_for(GameState::MainMenu, FlowIntent::ContinueRun),
            Some(GameState::Fight)
        );
    }

    /// #217: a save whose stored resume destination is the shop routes
    /// **Continuă** there instead of the arena.
    #[test]
    fn menu_continue_to_shop_routes_to_shop() {
        assert_eq!(
            transition_for(GameState::MainMenu, FlowIntent::ContinueToShop),
            Some(GameState::Shop)
        );
    }

    /// #129: a save whose stored resume destination is the town hub (the
    /// default for every non-shop checkpoint) routes **Continuă** there.
    #[test]
    fn menu_continue_to_town_routes_to_town() {
        assert_eq!(
            transition_for(GameState::MainMenu, FlowIntent::ContinueToTown),
            Some(GameState::Town)
        );
    }

    /// #129: a confirmed hero starts the run at the town hub, not straight
    /// in the arena.
    #[test]
    fn creation_confirm_hero_routes_to_town() {
        assert_eq!(
            transition_for(GameState::CharacterCreation, FlowIntent::ConfirmHero),
            Some(GameState::Town)
        );
    }

    #[test]
    fn creation_back_to_menu_routes_to_main_menu() {
        assert_eq!(
            transition_for(GameState::CharacterCreation, FlowIntent::BackToMenu),
            Some(GameState::MainMenu)
        );
    }

    /// #129: the hub's dominant primary action starts the next ladder fight.
    #[test]
    fn town_enter_arena_routes_to_fight() {
        assert_eq!(
            transition_for(GameState::Town, FlowIntent::EnterArena),
            Some(GameState::Fight)
        );
    }

    /// #129: the shop is an optional detour off the hub.
    #[test]
    fn town_go_to_shop_routes_to_shop() {
        assert_eq!(
            transition_for(GameState::Town, FlowIntent::GoToShop),
            Some(GameState::Shop)
        );
    }

    /// #129: the hub's back action returns to the main menu (the save is
    /// kept — the emitting screen owns that domain decision, not the table).
    #[test]
    fn town_back_to_menu_routes_to_main_menu() {
        assert_eq!(
            transition_for(GameState::Town, FlowIntent::BackToMenu),
            Some(GameState::MainMenu)
        );
    }

    /// #129: the result screen's single **Continuă** returns to the hub.
    #[test]
    fn fight_result_go_to_town_routes_to_town() {
        assert_eq!(
            transition_for(GameState::FightResult, FlowIntent::GoToTown),
            Some(GameState::Town)
        );
    }

    /// #129: leaving the shop returns to the hub, not the arena.
    #[test]
    fn shop_go_to_town_routes_to_town() {
        assert_eq!(
            transition_for(GameState::Shop, FlowIntent::GoToTown),
            Some(GameState::Town)
        );
    }

    /// #129: the next lap loops through the hub (the shop is optional now).
    #[test]
    fn victory_next_lap_routes_to_town() {
        assert_eq!(
            transition_for(GameState::Victory, FlowIntent::NextLap),
            Some(GameState::Town)
        );
    }

    #[test]
    fn victory_back_to_menu_routes_to_main_menu() {
        assert_eq!(
            transition_for(GameState::Victory, FlowIntent::BackToMenu),
            Some(GameState::MainMenu)
        );
    }

    #[test]
    fn game_over_back_to_menu_routes_to_main_menu() {
        assert_eq!(
            transition_for(GameState::GameOver, FlowIntent::BackToMenu),
            Some(GameState::MainMenu)
        );
    }

    #[test]
    fn paused_fight_abandon_routes_to_main_menu() {
        assert_eq!(
            transition_for(GameState::Fight, FlowIntent::AbandonFight),
            Some(GameState::MainMenu)
        );
    }

    #[test]
    fn fight_resolve_victory_routes_to_fight_result() {
        assert_eq!(
            transition_for(GameState::Fight, FlowIntent::ResolveVictory),
            Some(GameState::FightResult)
        );
    }

    #[test]
    fn fight_resolve_defeat_routes_to_game_over() {
        assert_eq!(
            transition_for(GameState::Fight, FlowIntent::ResolveDefeat),
            Some(GameState::GameOver)
        );
    }

    #[test]
    fn fight_run_won_routes_to_victory() {
        assert_eq!(
            transition_for(GameState::Fight, FlowIntent::RunWon),
            Some(GameState::Victory)
        );
    }

    /// Every row of the table this slice owns, and nothing else — a stray
    /// extra row (e.g. accidentally letting `ConfirmHero` apply from
    /// `MainMenu`) would defeat the point of a validated table.
    #[test]
    fn table_covers_exactly_the_owned_rows() {
        let owned = [
            (GameState::MainMenu, FlowIntent::StartNewGame),
            (GameState::MainMenu, FlowIntent::ContinueRun),
            (GameState::MainMenu, FlowIntent::ContinueToShop),
            (GameState::MainMenu, FlowIntent::ContinueToTown),
            (GameState::CharacterCreation, FlowIntent::ConfirmHero),
            (GameState::CharacterCreation, FlowIntent::BackToMenu),
            (GameState::Town, FlowIntent::EnterArena),
            (GameState::Town, FlowIntent::GoToShop),
            (GameState::Town, FlowIntent::BackToMenu),
            (GameState::FightResult, FlowIntent::GoToTown),
            (GameState::Shop, FlowIntent::GoToTown),
            (GameState::Victory, FlowIntent::NextLap),
            (GameState::Victory, FlowIntent::BackToMenu),
            (GameState::GameOver, FlowIntent::BackToMenu),
            (GameState::Fight, FlowIntent::AbandonFight),
            (GameState::Fight, FlowIntent::ResolveVictory),
            (GameState::Fight, FlowIntent::ResolveDefeat),
            (GameState::Fight, FlowIntent::RunWon),
        ];
        let all_states = [
            GameState::Loading,
            GameState::MainMenu,
            GameState::CharacterCreation,
            GameState::Town,
            GameState::Shop,
            GameState::Fight,
            GameState::FightResult,
            GameState::GameOver,
            GameState::Victory,
        ];
        let all_intents = [
            FlowIntent::StartNewGame,
            FlowIntent::ContinueRun,
            FlowIntent::ContinueToShop,
            FlowIntent::ContinueToTown,
            FlowIntent::ConfirmHero,
            FlowIntent::BackToMenu,
            FlowIntent::GoToShop,
            FlowIntent::EnterArena,
            FlowIntent::GoToTown,
            FlowIntent::NextLap,
            FlowIntent::AbandonFight,
            FlowIntent::ResolveVictory,
            FlowIntent::ResolveDefeat,
            FlowIntent::RunWon,
        ];
        for state in all_states {
            for intent in all_intents {
                let expects_row = owned.contains(&(state, intent));
                assert_eq!(
                    transition_for(state, intent).is_some(),
                    expects_row,
                    "unexpected table entry for ({state:?}, {intent:?})"
                );
            }
        }
    }

    // --- Invalid / duplicate intents are deterministic no-ops ---

    #[test]
    fn an_intent_invalid_for_the_current_state_is_rejected_and_leaves_state_unchanged() {
        let mut app = test_app();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu
        );
        write_intent(&mut app, FlowIntent::ConfirmHero);
        app.update();

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu,
            "invalid intent must not move state"
        );
        assert!(
            next_state_is_unchanged(&app),
            "invalid intent must not queue a transition"
        );
        assert_eq!(
            results(&mut app),
            vec![TransitionResult::Rejected {
                intent: FlowIntent::ConfirmHero,
                from: GameState::MainMenu,
            }]
        );
    }

    #[test]
    fn duplicate_intents_in_the_same_frame_apply_once_and_reject_the_rest() {
        let mut app = test_app();
        // Two identical StartNewGame intents queued before a single update:
        // the first is valid from MainMenu; the second is now invalid
        // because the *effective* state has already advanced to
        // CharacterCreation this frame.
        write_intent(&mut app, FlowIntent::StartNewGame);
        write_intent(&mut app, FlowIntent::StartNewGame);
        app.update();

        assert!(
            next_state_is_pending(&app, GameState::CharacterCreation),
            "exactly one transition is queued"
        );
        assert_eq!(
            results(&mut app),
            vec![
                TransitionResult::Applied {
                    intent: FlowIntent::StartNewGame,
                    from: GameState::MainMenu,
                    to: GameState::CharacterCreation,
                },
                TransitionResult::Rejected {
                    intent: FlowIntent::StartNewGame,
                    from: GameState::CharacterCreation,
                },
            ]
        );
    }

    #[test]
    fn a_valid_intent_queues_next_state_the_same_frame_it_is_applied() {
        let mut app = test_app();
        write_intent(&mut app, FlowIntent::StartNewGame);
        app.update();
        assert!(next_state_is_pending(&app, GameState::CharacterCreation));
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::CharacterCreation
        );
    }

    #[test]
    fn back_to_menu_from_character_creation_is_valid() {
        let mut app = test_app();
        set_state(&mut app, GameState::CharacterCreation);
        write_intent(&mut app, FlowIntent::BackToMenu);
        app.update();
        assert!(next_state_is_pending(&app, GameState::MainMenu));
    }

    /// A duplicate post-fight intent (e.g. two clicks landing in the same
    /// frame) must apply once and deterministically reject the rest, exactly
    /// like the menu/creation duplicate case above.
    #[test]
    fn duplicate_post_fight_intents_in_the_same_frame_apply_once_and_reject_the_rest() {
        let mut app = test_app();
        set_state(&mut app, GameState::FightResult);
        write_intent(&mut app, FlowIntent::GoToTown);
        write_intent(&mut app, FlowIntent::GoToTown);
        app.update();

        assert!(
            next_state_is_pending(&app, GameState::Town),
            "exactly one transition is queued"
        );
        assert_eq!(
            results(&mut app),
            vec![
                TransitionResult::Applied {
                    intent: FlowIntent::GoToTown,
                    from: GameState::FightResult,
                    to: GameState::Town,
                },
                TransitionResult::Rejected {
                    intent: FlowIntent::GoToTown,
                    from: GameState::Town,
                },
            ]
        );
    }

    /// An intent valid from a different owned state (e.g. `AbandonFight`,
    /// owned only from `Fight`) is rejected outside its state, same as any
    /// other invalid pairing.
    #[test]
    fn abandon_fight_is_invalid_outside_the_fight_state() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);
        write_intent(&mut app, FlowIntent::AbandonFight);
        app.update();

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Shop,
            "invalid intent must not move state"
        );
        assert_eq!(
            results(&mut app),
            vec![TransitionResult::Rejected {
                intent: FlowIntent::AbandonFight,
                from: GameState::Shop,
            }]
        );
    }

    // --- #166: automated combat-outcome intents ---

    #[test]
    fn resolve_defeat_is_invalid_outside_the_fight_state() {
        let mut app = test_app();
        set_state(&mut app, GameState::Shop);
        write_intent(&mut app, FlowIntent::ResolveDefeat);
        app.update();

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Shop,
            "invalid intent must not move state"
        );
        assert_eq!(
            results(&mut app),
            vec![TransitionResult::Rejected {
                intent: FlowIntent::ResolveDefeat,
                from: GameState::Shop,
            }]
        );
    }

    /// A duplicate automated intent (e.g. `tick_fight_end_delay` somehow
    /// firing twice in one frame) applies once and deterministically rejects
    /// the rest — the same generic duplicate-rejection behavior every other
    /// intent gets from `apply_flow_intents`, exercised here for the
    /// automated family specifically.
    #[test]
    fn duplicate_automated_resolve_victory_intents_in_the_same_frame_apply_once_and_reject_the_rest()
     {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        write_intent(&mut app, FlowIntent::ResolveVictory);
        write_intent(&mut app, FlowIntent::ResolveVictory);
        app.update();

        assert!(
            next_state_is_pending(&app, GameState::FightResult),
            "exactly one transition is queued"
        );
        assert_eq!(
            results(&mut app),
            vec![
                TransitionResult::Applied {
                    intent: FlowIntent::ResolveVictory,
                    from: GameState::Fight,
                    to: GameState::FightResult,
                },
                TransitionResult::Rejected {
                    intent: FlowIntent::ResolveVictory,
                    from: GameState::FightResult,
                },
            ]
        );
    }

    /// A stale automated intent — one that would have been valid at the
    /// start of the frame but is no longer, because an earlier intent this
    /// same frame already moved the *effective* state — is rejected exactly
    /// like any other invalid pairing. This is the scenario the fight-end
    /// delay and a player's pause-menu abandon could race into: both queue
    /// in the same frame, the first read wins, and the second is a
    /// deterministic no-op rather than clobbering the first.
    #[test]
    fn a_stale_automated_intent_after_an_earlier_transition_in_the_same_frame_is_rejected() {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        write_intent(&mut app, FlowIntent::AbandonFight);
        write_intent(&mut app, FlowIntent::ResolveVictory);
        app.update();

        assert!(
            next_state_is_pending(&app, GameState::MainMenu),
            "only the first-read intent (AbandonFight) is queued"
        );
        assert_eq!(
            results(&mut app),
            vec![
                TransitionResult::Applied {
                    intent: FlowIntent::AbandonFight,
                    from: GameState::Fight,
                    to: GameState::MainMenu,
                },
                TransitionResult::Rejected {
                    intent: FlowIntent::ResolveVictory,
                    from: GameState::MainMenu,
                },
            ]
        );
    }

    // --- Full headless journeys ---
    //
    // These drive the whole state machine end-to-end through FlowIntents
    // only (the harness pattern from #155/#164), extended here per #166 to
    // cover the automated combat-outcome routes. Domain side effects (reward
    // crediting, run reset, save requests) are covered where they live —
    // `progression::result_ui`/`victory_ui`/`mod`, `shop`, `combat::pause` —
    // this module only asserts the *routing* holds across a whole journey.

    /// menu -> creation -> town -> fight -> result -> town -> next fight:
    /// the complete first-lap loop a fresh run takes when every fight is won
    /// (#129: the town hub sits between every fight, and the shop is an
    /// optional detour exercised in the next journey).
    #[test]
    fn journey_menu_to_creation_to_town_to_fight_to_result_to_town_to_next_fight() {
        let mut app = test_app();
        assert_eq!(current_state(&app), GameState::MainMenu);
        assert_eq!(
            step(&mut app, FlowIntent::StartNewGame),
            GameState::CharacterCreation
        );
        assert_eq!(
            step(&mut app, FlowIntent::ConfirmHero),
            GameState::Town,
            "a confirmed hero starts the run at the hub"
        );
        assert_eq!(step(&mut app, FlowIntent::EnterArena), GameState::Fight);
        assert_eq!(
            step(&mut app, FlowIntent::ResolveVictory),
            GameState::FightResult,
            "the automated fight-end delay routes a non-final win to the result screen"
        );
        assert_eq!(
            step(&mut app, FlowIntent::GoToTown),
            GameState::Town,
            "the result screen's Continuă returns to the hub"
        );
        assert_eq!(
            step(&mut app, FlowIntent::EnterArena),
            GameState::Fight,
            "the hub starts the next fight"
        );
    }

    /// town -> shop -> town -> fight: #129's optional shop detour. The shop
    /// is entered from the hub and leaving it lands back on the hub, never
    /// straight in the arena.
    #[test]
    fn journey_town_shop_detour_returns_to_town() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);
        assert_eq!(step(&mut app, FlowIntent::GoToShop), GameState::Shop);
        assert_eq!(
            step(&mut app, FlowIntent::GoToTown),
            GameState::Town,
            "leaving the shop returns to the hub"
        );
        assert_eq!(step(&mut app, FlowIntent::EnterArena), GameState::Fight);
    }

    /// menu -> shop -> town: #217's other **Continuă** destination. A save
    /// captured at a shop checkpoint resumes straight into the shop, and
    /// leaving it continues through the hub like any other shop visit.
    #[test]
    fn journey_menu_continue_to_shop_then_back_to_town() {
        let mut app = test_app();
        assert_eq!(
            step(&mut app, FlowIntent::ContinueToShop),
            GameState::Shop,
            "Continuă resumes into the shop when that's the saved destination"
        );
        assert_eq!(
            step(&mut app, FlowIntent::GoToTown),
            GameState::Town,
            "leaving the shop returns to the hub, same as any other shop visit"
        );
    }

    /// menu -> town -> menu: #129's **Continuă** default. A save captured at
    /// any non-shop checkpoint resumes into the hub, and the hub's back
    /// action returns to the menu with the save kept (the table only routes;
    /// the emitting screen owns the confirm overlay and keeps the save).
    #[test]
    fn journey_menu_continue_to_town_and_back() {
        let mut app = test_app();
        assert_eq!(
            step(&mut app, FlowIntent::ContinueToTown),
            GameState::Town,
            "Continuă resumes into the hub when that's the saved destination"
        );
        assert_eq!(step(&mut app, FlowIntent::BackToMenu), GameState::MainMenu);
    }

    /// defeat -> game over -> reset: a loss ends the run and the game-over
    /// screen's back-to-menu button (which also resets the run, in
    /// `progression::result_ui::handle_game_over_actions`) returns to the
    /// main menu.
    #[test]
    fn journey_defeat_to_game_over_to_reset() {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        assert_eq!(
            step(&mut app, FlowIntent::ResolveDefeat),
            GameState::GameOver,
            "the automated fight-end delay routes a loss to game over"
        );
        assert_eq!(
            step(&mut app, FlowIntent::BackToMenu),
            GameState::MainMenu,
            "game over's back-to-menu resets the run and returns to the menu"
        );
    }

    /// victory -> next lap: winning the lap-1 final boss ends the run in
    /// victory, and continuing loops into lap 2 via the town hub (#129).
    #[test]
    fn journey_victory_to_next_lap() {
        let mut app = test_app();
        set_state(&mut app, GameState::Fight);
        assert_eq!(
            step(&mut app, FlowIntent::RunWon),
            GameState::Victory,
            "the automated fight-end delay routes the run-ending win to victory"
        );
        assert_eq!(
            step(&mut app, FlowIntent::NextLap),
            GameState::Town,
            "continuing the run loops into lap 2 via the hub"
        );
    }

    // --- #166: sole-owner regression check ---

    /// #166's acceptance criterion: `FlowPlugin` is the only runtime owner
    /// of `NextState<GameState>`. `ownership_scan` (below) parses every
    /// `src/**/*.rs` file with `syn` and flags any production (non-
    /// `#[cfg(test)]`) function whose signature or body touches
    /// `NextState<GameState>`. This asserts every such finding outside
    /// `src/flow/` is exactly the one documented, pre-existing exception:
    /// `core::transition_out_of_loading`, the asset-readiness bootstrap gate
    /// from #114 that predates the #142 flow-intent effort. `Loading` is not
    /// a screen or gameplay state this table routes (see the module docs'
    /// transition table, which has no `Loading` row) — it is a one-time
    /// "assets ready?" poll that runs once at startup, not a player- or
    /// gameplay-triggered navigation route, so it is out of #142/#166's
    /// scope rather than a second competing owner of the routes this table
    /// does own. A reintroduced direct write anywhere else (e.g. a future
    /// `tick_fight_end_delay`-style shortcut) fails this test.
    ///
    /// The exact-count assertion (not just "the exception is present, and
    /// nothing else is unexpected") matters: filtering findings down to
    /// "not equal to the documented exception" alone couldn't distinguish a
    /// second, different writer that happened to share the exact same file
    /// path and function name (e.g. two `transition_out_of_loading`
    /// functions in different inner modules of `src/core/mod.rs`) from the
    /// real exception — asserting `findings.len() == 1` closes that gap.
    #[test]
    fn flow_plugin_is_the_sole_runtime_next_state_game_state_writer() {
        let findings = super::ownership_scan::find_non_flow_next_state_writers();
        let documented_exception = super::ownership_scan::Finding {
            file: "src/core/mod.rs".to_string(),
            item: "transition_out_of_loading".to_string(),
        };
        assert_eq!(
            findings,
            vec![documented_exception],
            "the only runtime NextState<GameState> writer outside src/flow/ should be the \
             documented Loading-gate exception (core::transition_out_of_loading) — anything \
             else here is either a regression or a legitimate new exception that needs this \
             test's allowlist (and the module docs' transition table) updated deliberately"
        );
    }
}

/// Structural (AST-based, not textual) scan backing
/// `flow_plugin_is_the_sole_runtime_next_state_game_state_writer`. Parses
/// every `src/**/*.rs` file with `syn` — rather than grepping for the
/// substring `"NextState<GameState>"`, which cannot tell a real system
/// parameter from a doc comment, a string literal, or dead code behind an
/// unrelated `cfg` — and looks for:
///
///   - a function parameter (free function or `impl` method) whose type
///     mentions `NextState<GameState>` anywhere in its generic arguments,
///     which catches `ResMut<NextState<GameState>>`,
///     `Option<ResMut<NextState<GameState>>>`, and any future wrapper,
///     since every real writer in this codebase (the sole owner included)
///     takes it as a plain system parameter;
///   - a `.resource_mut::<NextState<GameState>>()` turbofish call, in case a
///     future writer reaches for exclusive-world access instead.
///
/// Anything reachable only through a `#[cfg(test)]`- (or `#[test]`-)gated
/// item — a whole `mod`, `impl` block, or `fn` — is test-only and skipped,
/// since it never runs in the shipped game.
///
/// **Limits** (documented per #166's ask, since no static check like this is
/// exhaustive): this only recognizes the literal `#[cfg(test)]`/`#[test]`
/// attributes actually used in this repo — not general predicates like
/// `cfg(any(test, feature = "x"))`, so a hypothetical future writer gated
/// behind, say, `#[cfg(feature = "dev")]` (a debug/cheat system, not a test)
/// would read as production code and correctly still be flagged, but would
/// need its own deliberate allowlist entry rather than being swept under the
/// existing test-only exemption; it does no name/type-alias resolution, so a
/// hypothetical `type NsGs = NextState<GameState>;` would slip past; and it
/// only understands free functions and `impl`-block methods, not a
/// `NextState<GameState>` reached through a custom `#[derive(SystemParam)]`
/// wrapper struct's fields. A file that fails to read or parse panics rather
/// than being silently skipped (see `find_non_flow_next_state_writers`), so
/// at least that failure mode is loud. None of these gaps are exercised
/// anywhere in this codebase today.
#[cfg(test)]
mod ownership_scan {
    use std::fs;
    use std::path::{Path, PathBuf};
    use syn::visit::{self, Visit};
    use syn::{Attribute, FnArg, GenericArgument, ImplItemFn, ItemFn, PathArguments, Type};

    /// One production function whose signature touches
    /// `NextState<GameState>`, found outside `src/flow/`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Finding {
        pub file: String,
        pub item: String,
    }

    struct Scanner {
        file: String,
        cfg_test_depth: usize,
        findings: Vec<Finding>,
    }

    /// True for `#[cfg(test)]` or bare `#[test]` — the only two forms this
    /// codebase uses to mark test-only code.
    fn is_cfg_test(attrs: &[Attribute]) -> bool {
        attrs.iter().any(|attr| {
            if attr.path().is_ident("test") {
                return true;
            }
            if !attr.path().is_ident("cfg") {
                return false;
            }
            attr.parse_args::<syn::Path>()
                .map(|path| path.is_ident("test"))
                .unwrap_or(false)
        })
    }

    /// True if `ty` mentions `NextState<GameState>` anywhere in its generic
    /// arguments — recurses through any wrapper (`ResMut<...>`,
    /// `Option<...>`, ...) regardless of the outer type's name, so it does
    /// not need to know every wrapper this or a future writer might use.
    fn mentions_next_state_of_game_state(ty: &Type) -> bool {
        let Type::Path(type_path) = ty else {
            return false;
        };
        for segment in &type_path.path.segments {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                continue;
            };
            if segment.ident == "NextState" {
                let has_game_state = args.args.iter().any(|arg| {
                    matches!(
                        arg,
                        GenericArgument::Type(Type::Path(inner))
                            if inner.path.segments.last().is_some_and(|s| s.ident == "GameState")
                    )
                });
                if has_game_state {
                    return true;
                }
            }
            let recurses_into_generics = args.args.iter().any(|arg| {
                matches!(arg, GenericArgument::Type(inner) if mentions_next_state_of_game_state(inner))
            });
            if recurses_into_generics {
                return true;
            }
        }
        false
    }

    fn signature_writes_next_state(
        inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    ) -> bool {
        inputs.iter().any(|arg| match arg {
            FnArg::Typed(pat_type) => mentions_next_state_of_game_state(&pat_type.ty),
            FnArg::Receiver(_) => false,
        })
    }

    impl Scanner {
        /// Enters an item: bumps `cfg_test_depth` if `attrs` carries
        /// `#[cfg(test)]`/`#[test]`, and returns whether it did (so the
        /// matching `exit` call knows whether to undo it). Shared by every
        /// `visit_item_*`/`visit_impl_item_*` override below so the
        /// depth-tracking bookkeeping lives in exactly one place.
        fn enter(&mut self, attrs: &[Attribute]) -> bool {
            let entering_test = is_cfg_test(attrs);
            if entering_test {
                self.cfg_test_depth += 1;
            }
            entering_test
        }

        /// Undoes `enter`'s depth bump, if it made one.
        fn exit(&mut self, entered_test: bool) {
            if entered_test {
                self.cfg_test_depth -= 1;
            }
        }

        /// Records a finding if `sig` is production code (depth 0) and its
        /// parameters touch `NextState<GameState>`. Shared by the free-`fn`
        /// and `impl`-method visitors, which differ only in which AST node
        /// carries the `Signature`.
        fn record_if_writer(&mut self, sig: &syn::Signature) {
            if self.cfg_test_depth == 0 && signature_writes_next_state(&sig.inputs) {
                self.findings.push(Finding {
                    file: self.file.clone(),
                    item: sig.ident.to_string(),
                });
            }
        }
    }

    impl<'ast> Visit<'ast> for Scanner {
        fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
            let entered_test = self.enter(&node.attrs);
            visit::visit_item_mod(self, node);
            self.exit(entered_test);
        }

        fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
            let entered_test = self.enter(&node.attrs);
            visit::visit_item_impl(self, node);
            self.exit(entered_test);
        }

        fn visit_item_fn(&mut self, node: &'ast ItemFn) {
            let entered_test = self.enter(&node.attrs);
            self.record_if_writer(&node.sig);
            visit::visit_item_fn(self, node);
            self.exit(entered_test);
        }

        fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
            let entered_test = self.enter(&node.attrs);
            self.record_if_writer(&node.sig);
            visit::visit_impl_item_fn(self, node);
            self.exit(entered_test);
        }

        fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
            if self.cfg_test_depth == 0
                && node.method == "resource_mut"
                && let Some(turbofish) = &node.turbofish
            {
                let hits = turbofish.args.iter().any(|arg| {
                    matches!(arg, GenericArgument::Type(ty) if mentions_next_state_of_game_state(ty))
                });
                if hits {
                    self.findings.push(Finding {
                        file: self.file.clone(),
                        item: "resource_mut::<NextState<GameState>>() call".to_string(),
                    });
                }
            }
            visit::visit_expr_method_call(self, node);
        }
    }

    /// Recurses `dir` collecting every `.rs` file. Panics on a read failure
    /// instead of silently skipping the directory/entry — an unreadable
    /// directory under `src/` would otherwise make the scan quietly cover
    /// less than it claims to, which is exactly the wrong failure mode for
    /// an ownership guard.
    fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|err| panic!("ownership_scan: can't read {}: {err}", dir.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|err| {
                panic!(
                    "ownership_scan: can't read a directory entry under {}: {err}",
                    dir.display()
                )
            });
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// Scans every `src/**/*.rs` file for production (non-`#[cfg(test)]`)
    /// code whose signature or body touches `NextState<GameState>`, and
    /// returns the findings whose file is not under `src/flow/`. A file that
    /// can't be read or parsed panics rather than being silently skipped
    /// (see [`collect_rs_files`]) — a class this could otherwise miss
    /// unnoticed matters too much to fail quietly.
    pub fn find_non_flow_next_state_writers() -> Vec<Finding> {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs_files(&src_root, &mut files);
        assert!(
            files.len() > 20,
            "ownership_scan only found {} .rs file(s) under {} — that's suspiciously few for \
             this codebase, so the scan is probably broken rather than the codebase shrunk",
            files.len(),
            src_root.display()
        );
        let mut findings = Vec::new();
        for path in files {
            let contents = fs::read_to_string(&path).unwrap_or_else(|err| {
                panic!("ownership_scan: can't read {}: {err}", path.display())
            });
            let parsed = syn::parse_file(&contents).unwrap_or_else(|err| {
                panic!(
                    "ownership_scan: can't parse {} as Rust ({err}) — fix the file or extend \
                     this scan's documented limits instead of letting it skip silently",
                    path.display()
                )
            });
            let relative = path
                .strip_prefix(&src_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if relative.starts_with("flow/") {
                continue;
            }
            let mut scanner = Scanner {
                file: format!("src/{relative}"),
                cfg_test_depth: 0,
                findings: Vec::new(),
            };
            scanner.visit_file(&parsed);
            findings.extend(scanner.findings);
        }
        findings
    }
}
