//! ECS glue for the combat engine: the turn resource, the seeded RNG
//! resource, temporary keyboard input (1–4), the hardcoded enemy reply, and
//! the write-back of [`engine::resolve_action`] results onto `Health` and
//! `Stamina` components.

use bevy::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::character::{Attributes, EnemyFighter, Health, PlayerFighter, Stamina};
use crate::core::GameState;

use super::engine::{self, CombatAction, CombatEvent};

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
}

/// Seeded RNG that drives every combat roll; the engine never touches
/// `thread_rng`. [`setup_combat`] seeds it from the app clock unless one was
/// already provided (tests insert a fixed seed for deterministic duels).
#[derive(Resource, Debug, Clone)]
pub struct CombatRng(pub ChaCha8Rng);

/// The player's chosen action for this turn. Written by the temporary 1–4
/// keyboard mapping until the HUD issue replaces it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerActionEvent(pub CombatAction);

/// A combat event that occurred, tagged with who acted. The HUD log,
/// announcer, and FX issues consume these.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatLogEvent {
    /// Who performed the action that produced this event.
    pub actor: CombatSide,
    /// What happened.
    pub event: CombatEvent,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayerActionEvent>()
            .add_message::<CombatLogEvent>()
            .add_systems(OnEnter(GameState::Fight), setup_combat)
            .add_systems(OnExit(GameState::Fight), teardown_combat)
            .add_systems(
                Update,
                (init_turn, player_input, resolve_player_action, enemy_turn)
                    .chain()
                    .run_if(in_state(GameState::Fight)),
            );
    }
}

/// Query for the components the resolver reads and writes on one side.
type FighterComponents<'w, 's, Side, OtherSide> = Query<
    'w,
    's,
    (
        &'static Attributes,
        &'static mut Health,
        &'static mut Stamina,
    ),
    (With<Side>, Without<OtherSide>),
>;
type PlayerQuery<'w, 's> = FighterComponents<'w, 's, PlayerFighter, EnemyFighter>;
type EnemyQuery<'w, 's> = FighterComponents<'w, 's, EnemyFighter, PlayerFighter>;

/// Seeds the duel RNG from the app clock — unless a [`CombatRng`] already
/// exists, so tests (or a future daily-seed mode) can provide their own.
fn setup_combat(mut commands: Commands, time: Res<Time>, rng: Option<Res<CombatRng>>) {
    if rng.is_none() {
        commands.insert_resource(CombatRng(ChaCha8Rng::seed_from_u64(
            time.elapsed().as_micros() as u64,
        )));
    }
}

/// Drops the duel state so the next fight starts fresh.
fn teardown_combat(mut commands: Commands) {
    commands.remove_resource::<CombatTurn>();
    commands.remove_resource::<CombatRng>();
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
    });
}

/// Temporary keyboard mapping until the HUD issue lands: 1 = quick strike,
/// 2 = heavy strike, 3 = block, 4 = rest. Only listens on the player's turn
/// while the duel is running.
fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    turn: Option<Res<CombatTurn>>,
    mut actions: MessageWriter<PlayerActionEvent>,
) {
    let Some(turn) = turn else {
        return;
    };
    if turn.side != CombatSide::Player || turn.over {
        return;
    }
    let mappings = [
        (KeyCode::Digit1, CombatAction::QuickStrike),
        (KeyCode::Digit2, CombatAction::HeavyStrike),
        (KeyCode::Digit3, CombatAction::Block),
        (KeyCode::Digit4, CombatAction::Rest),
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
    mut log: MessageWriter<CombatLogEvent>,
    mut player: PlayerQuery,
    mut enemy: EnemyQuery,
) {
    let (Some(mut turn), Some(mut rng)) = (turn, rng) else {
        return;
    };
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
    }
}

/// The hardcoded enemy reply — always a quick strike — until the AI issue
/// replaces it. Runs in the same frame right after the player resolves.
fn enemy_turn(
    turn: Option<ResMut<CombatTurn>>,
    rng: Option<ResMut<CombatRng>>,
    mut log: MessageWriter<CombatLogEvent>,
    mut player: PlayerQuery,
    mut enemy: EnemyQuery,
) {
    let (Some(mut turn), Some(mut rng)) = (turn, rng) else {
        return;
    };
    if turn.side != CombatSide::Enemy || turn.over {
        return;
    }
    let (Ok(player), Ok(enemy)) = (player.single_mut(), enemy.single_mut()) else {
        return;
    };
    apply_action(
        CombatAction::QuickStrike,
        CombatSide::Enemy,
        enemy,
        player,
        &mut turn,
        &mut rng,
        &mut log,
    );
}

/// One side's components as yielded by a [`FighterComponents`] query.
type FighterItem<'a> = (&'a Attributes, Mut<'a, Health>, Mut<'a, Stamina>);

/// Snapshots both fighters into pure [`engine::FighterState`]s, resolves the
/// action, writes the results back to the components and the turn resource,
/// logs every event, and passes the turn — ending the duel on `Defeated`.
///
/// The turn passes even on an [`CombatEvent::OutOfStamina`] no-op so the
/// hardcoded enemy can never wedge the duel by re-trying a strike forever.
fn apply_action(
    action: CombatAction,
    actor_side: CombatSide,
    actor: FighterItem,
    target: FighterItem,
    turn: &mut CombatTurn,
    rng: &mut CombatRng,
    log: &mut MessageWriter<CombatLogEvent>,
) {
    let (actor_attrs, mut actor_hp, mut actor_stamina) = actor;
    let (target_attrs, mut target_hp, mut target_stamina) = target;
    let (actor_blocking, target_blocking) = match actor_side {
        CombatSide::Player => (&mut turn.player_blocking, &mut turn.enemy_blocking),
        CombatSide::Enemy => (&mut turn.enemy_blocking, &mut turn.player_blocking),
    };
    let mut actor_state = engine::FighterState {
        hp: actor_hp.current,
        stamina: actor_stamina.current,
        attributes: *actor_attrs,
        blocking: *actor_blocking,
    };
    let mut target_state = engine::FighterState {
        hp: target_hp.current,
        stamina: target_stamina.current,
        attributes: *target_attrs,
        blocking: *target_blocking,
    };

    let events = engine::resolve_action(&mut actor_state, &mut target_state, action, &mut rng.0);

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
            event,
        });
    }
    turn.side = actor_side.opponent();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::character::stats::{CRIT_PERCENT_CAP, HIT_PERCENT_MIN};
    use crate::core::CorePlugin;
    use crate::creation::PlayerCharacter;
    use bevy::state::app::StatesPlugin;
    use rand::RngExt as _;

    /// Same player build as the arena tests: putere 4 (damage 6), agilitate
    /// 2 (ties the Strigoi), vitalitate 4 (90 hp, 50 stamina), noroc 3.
    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    /// Headless app on the fight screen with a deterministic duel RNG whose
    /// first four strikes are clean hits without crits (see [`strikes_rng`]).
    fn test_app() -> App {
        test_app_with(PLAYER_ATTRIBUTES)
    }

    fn test_app_with(attributes: Attributes) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes,
        });
        app.insert_resource(CombatRng(strikes_rng(4)));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update(); // transition + OnEnter + first combat frame
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

    #[test]
    fn the_turn_opens_with_the_player_on_an_agility_tie() {
        let mut app = test_app(); // player agilitate 2 vs Strigoi 2
        assert_eq!(
            turn(&app),
            CombatTurn {
                side: CombatSide::Player,
                over: false,
                player_blocking: false,
                enemy_blocking: false,
            }
        );
        assert_eq!(enemy_pools(&mut app), (70, 40), "enemy untouched");
    }

    #[test]
    fn a_faster_enemy_opens_the_round_and_strikes_immediately() {
        let mut app = test_app_with(Attributes {
            agilitate: 1, // Strigoi has 2
            ..PLAYER_ATTRIBUTES
        });
        // The enemy quick-struck (hit, no crit, damage 4) and passed the turn.
        assert_eq!(turn(&app).side, CombatSide::Player);
        assert_eq!(player_pools(&mut app), (86, 50), "90 hp - 4 damage");
        assert_eq!(enemy_pools(&mut app), (70, 35), "quick strike costs 5");
    }

    #[test]
    fn key_one_quick_strikes_and_the_enemy_replies_in_kind() {
        let mut app = test_app();
        press(&mut app, KeyCode::Digit1);
        // Player: hit for base damage 6, -5 stamina. Enemy reply: hit for
        // base damage 4, -5 stamina. Turn is back with the player.
        assert_eq!(enemy_pools(&mut app), (64, 35));
        assert_eq!(player_pools(&mut app), (86, 45));
        assert_eq!(turn(&app).side, CombatSide::Player);
    }

    #[test]
    fn key_two_heavy_strikes_for_double_damage() {
        let mut app = test_app();
        press(&mut app, KeyCode::Digit2);
        assert_eq!(enemy_pools(&mut app), (58, 35), "70 hp - 2 * 6 damage");
        assert_eq!(player_pools(&mut app), (86, 35), "heavy strike costs 15");
    }

    #[test]
    fn key_three_blocks_and_halves_the_enemy_reply() {
        let mut app = test_app();
        press(&mut app, KeyCode::Digit3);
        // The enemy replied into the guard: 4 damage halved to 2. The guard
        // holds until the player's next turn.
        assert_eq!(
            player_pools(&mut app),
            (88, 47),
            "90 - 2 hp, 50 - 3 stamina"
        );
        assert!(turn(&app).player_blocking);
        press(&mut app, KeyCode::Digit4);
        assert!(!turn(&app).player_blocking, "guard lapses on the next turn");
    }

    #[test]
    fn key_four_rests_stamina_back_up_to_the_cap() {
        let mut app = test_app();
        for _ in 0..3 {
            press(&mut app, KeyCode::Digit2); // 3 heavy strikes: 50 -> 5
        }
        assert_eq!(player_pools(&mut app).1, 5);
        press(&mut app, KeyCode::Digit4);
        assert_eq!(player_pools(&mut app).1, 25, "rest restores 20");
        press(&mut app, KeyCode::Digit4);
        press(&mut app, KeyCode::Digit4);
        assert_eq!(player_pools(&mut app).1, 50, "capped at max stamina");
    }

    #[test]
    fn a_strike_without_stamina_is_a_no_op_but_passes_the_turn() {
        let mut app = test_app();
        for _ in 0..3 {
            press(&mut app, KeyCode::Digit2); // 50 -> 5 stamina
        }
        assert_eq!(player_pools(&mut app).1, 5);
        let enemy_hp = enemy_pools(&mut app).0;
        press(&mut app, KeyCode::Digit2); // needs 15, has 5
        assert_eq!(enemy_pools(&mut app).0, enemy_hp, "no damage dealt");
        assert_eq!(player_pools(&mut app).1, 5, "no stamina spent");
        assert_eq!(
            enemy_pools(&mut app).1,
            20,
            "the turn still passed: the enemy paid for its fourth reply"
        );
    }

    #[test]
    fn defeat_ends_the_duel_and_stops_combat_input() {
        let mut app = test_app();
        // Quick-strike the enemy down (resting when out of stamina); the
        // enemy runs dry after 8 replies, so it can never win this race.
        for _ in 0..60 {
            if turn(&app).over {
                break;
            }
            let key = if player_pools(&mut app).1 >= 5 {
                KeyCode::Digit1
            } else {
                KeyCode::Digit4
            };
            press(&mut app, key);
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
}
