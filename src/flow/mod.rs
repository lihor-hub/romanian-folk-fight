//! Flow plugin (#155, #164): the single validated table of [`GameState`]
//! transitions for menu/creation navigation and the player-triggered routes
//! out of a fight (result, shop, victory, game-over, and paused-fight
//! abandon). Screens keep their domain side effects (run reset, hero/loadout
//! creation, save restore, rewards, purchases) but emit a [`FlowIntent`] for
//! navigation instead of writing `NextState<GameState>` directly.
//! [`apply_flow_intents`] is the only system that writes
//! `NextState<GameState>` for the routes this slice owns.
//!
//! Automated combat-outcome transitions (fight-end detection in
//! `combat::systems`/`progression`) are not migrated here — they keep
//! writing `NextState<GameState>` directly until a later #142 child brings
//! them under the same table.

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
    /// Menu → its current v1 destination (Fight): resume a restored save.
    /// The save must already be restored into the run resources.
    ContinueRun,
    /// Creation → Fight: the hero/loadout is confirmed and stored.
    ConfirmHero,
    /// Creation → menu, game-over → menu (with the run reset), or victory →
    /// menu (with the looping run's save kept): every "back to the main
    /// menu" button shares this intent since the table only routes state —
    /// the emitting screen already applied whatever domain effect (or none)
    /// its own destination implies.
    BackToMenu,
    /// FightResult → Shop: spend the payout (**La prăvălie**). The reward
    /// was already credited on `OnEnter(FightResult)`.
    GoToShop,
    /// FightResult → Fight: straight into the next duel (**Lupta
    /// următoare**).
    NextFight,
    /// Shop → Fight: leave the shop (**Înapoi în arenă**). Purchases/equips
    /// already applied as they were pressed.
    BackToArena,
    /// Victory → Shop: continue the looping run into lap 2 (**Turul 2**).
    /// The ladder already advanced past the last lap-1 opponent.
    NextLap,
    /// Paused Fight → menu: **Abandonează**. Not a defeat and does not touch
    /// the save — the run keeps its last autosave and the fight restarts
    /// fresh on return (#146 owns any future change to this policy).
    AbandonFight,
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

/// The transition table: exactly the routes owned by menu/creation (#155)
/// and by post-fight/pause navigation (#164). `None` covers every other
/// `(state, intent)` pair — including states and intents this slice
/// deliberately does not own (automated combat-outcome transitions).
fn transition_for(from: GameState, intent: FlowIntent) -> Option<GameState> {
    use FlowIntent::*;
    use GameState::*;
    match (from, intent) {
        (MainMenu, StartNewGame) => Some(CharacterCreation),
        (MainMenu, ContinueRun) => Some(Fight),
        (CharacterCreation, ConfirmHero) => Some(Fight),
        (CharacterCreation, BackToMenu) => Some(MainMenu),
        (FightResult, GoToShop) => Some(Shop),
        (FightResult, NextFight) => Some(Fight),
        (Shop, BackToArena) => Some(Fight),
        (Victory, NextLap) => Some(Shop),
        (Victory, BackToMenu) => Some(MainMenu),
        (GameOver, BackToMenu) => Some(MainMenu),
        (Fight, AbandonFight) => Some(MainMenu),
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

    #[test]
    fn creation_confirm_hero_routes_to_fight() {
        assert_eq!(
            transition_for(GameState::CharacterCreation, FlowIntent::ConfirmHero),
            Some(GameState::Fight)
        );
    }

    #[test]
    fn creation_back_to_menu_routes_to_main_menu() {
        assert_eq!(
            transition_for(GameState::CharacterCreation, FlowIntent::BackToMenu),
            Some(GameState::MainMenu)
        );
    }

    #[test]
    fn fight_result_go_to_shop_routes_to_shop() {
        assert_eq!(
            transition_for(GameState::FightResult, FlowIntent::GoToShop),
            Some(GameState::Shop)
        );
    }

    #[test]
    fn fight_result_next_fight_routes_to_fight() {
        assert_eq!(
            transition_for(GameState::FightResult, FlowIntent::NextFight),
            Some(GameState::Fight)
        );
    }

    #[test]
    fn shop_back_to_arena_routes_to_fight() {
        assert_eq!(
            transition_for(GameState::Shop, FlowIntent::BackToArena),
            Some(GameState::Fight)
        );
    }

    #[test]
    fn victory_next_lap_routes_to_shop() {
        assert_eq!(
            transition_for(GameState::Victory, FlowIntent::NextLap),
            Some(GameState::Shop)
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

    /// Every row of the table this slice owns, and nothing else — a stray
    /// extra row (e.g. accidentally letting `ConfirmHero` apply from
    /// `MainMenu`) would defeat the point of a validated table.
    #[test]
    fn table_covers_exactly_the_owned_rows() {
        let owned = [
            (GameState::MainMenu, FlowIntent::StartNewGame),
            (GameState::MainMenu, FlowIntent::ContinueRun),
            (GameState::CharacterCreation, FlowIntent::ConfirmHero),
            (GameState::CharacterCreation, FlowIntent::BackToMenu),
            (GameState::FightResult, FlowIntent::GoToShop),
            (GameState::FightResult, FlowIntent::NextFight),
            (GameState::Shop, FlowIntent::BackToArena),
            (GameState::Victory, FlowIntent::NextLap),
            (GameState::Victory, FlowIntent::BackToMenu),
            (GameState::GameOver, FlowIntent::BackToMenu),
            (GameState::Fight, FlowIntent::AbandonFight),
        ];
        let all_states = [
            GameState::Loading,
            GameState::MainMenu,
            GameState::CharacterCreation,
            GameState::Shop,
            GameState::Fight,
            GameState::FightResult,
            GameState::GameOver,
            GameState::Victory,
        ];
        let all_intents = [
            FlowIntent::StartNewGame,
            FlowIntent::ContinueRun,
            FlowIntent::ConfirmHero,
            FlowIntent::BackToMenu,
            FlowIntent::GoToShop,
            FlowIntent::NextFight,
            FlowIntent::BackToArena,
            FlowIntent::NextLap,
            FlowIntent::AbandonFight,
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
        write_intent(&mut app, FlowIntent::GoToShop);
        write_intent(&mut app, FlowIntent::GoToShop);
        app.update();

        assert!(
            next_state_is_pending(&app, GameState::Shop),
            "exactly one transition is queued"
        );
        assert_eq!(
            results(&mut app),
            vec![
                TransitionResult::Applied {
                    intent: FlowIntent::GoToShop,
                    from: GameState::FightResult,
                    to: GameState::Shop,
                },
                TransitionResult::Rejected {
                    intent: FlowIntent::GoToShop,
                    from: GameState::Shop,
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
}
