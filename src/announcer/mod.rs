//! The arena announcer: a louder, folk-humored layer over the same
//! [`CombatLogEvent`] stream the HUD log consumes. Every combat event (plus
//! the fight-start hook, and a boss-intro hook for the roster issue) picks a
//! Romanian one-liner from the static pools in [`lines`], substitutes the
//! fighter names and numbers, and shows it in a banner over the arena for
//! [`BANNER_SECONDS`] before fading out.
//!
//! Selection draws from the shared [`CombatRng`] and never repeats the
//! immediately previous line of the same pool ([`pick_index`]).

pub mod lines;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use rand::{Rng, RngExt as _};

use crate::character::{EnemyFighter, FighterName, PlayerFighter};
use crate::combat::{CombatEvent, CombatLogEvent, CombatRng, CombatSide};
use crate::core::{GameState, UiFont, despawn_screen};
use crate::roster::Boss;
use crate::theme::{BANNER_BACKGROUND, CREAM};

pub use lines::LineKey;

/// How long a banner line stays up without new events.
pub const BANNER_SECONDS: f32 = 2.5;
/// The tail of the banner's life over which it fades out.
const FADE_SECONDS: f32 = 0.4;

const BANNER_TOP: f32 = 64.0;

/// Marker for the banner strip root; despawns on `OnExit(GameState::Fight)`.
#[derive(Component)]
struct AnnouncerBanner;

/// The visibility-toggled panel holding the current line.
#[derive(Component)]
struct BannerPanel;

/// The banner's text node.
#[derive(Component)]
struct BannerText;

/// A request to announce something outside the combat-event stream: the
/// fight-start hook, and the [`LineKey::BossIntro`] written when the roster
/// spawns a [`Boss`] opponent.
#[derive(Message, Debug, Clone)]
pub struct AnnouncementRequest {
    /// Which pool to draw from.
    pub key: LineKey,
    /// Fills `{attacker}`/`{actor}`/`{winner}`.
    pub actor: String,
    /// Fills `{defender}`/`{loser}`/`{opponent}`.
    pub opponent: String,
    /// Fills `{dmg}`/`{amount}`.
    pub value: i32,
    /// A verbatim line shown instead of a pool draw (placeholders are still
    /// filled); bosses announce their own roster intro line this way.
    pub line: Option<String>,
}

/// Per-fight announcer bookkeeping: the last pick per pool (for the
/// no-immediate-repeat rule), the banner lifetime, the fight-start latch,
/// and the boss-intro hold that keeps the intro on screen for its full
/// lifetime even while the opening exchange already logs combat events.
#[derive(Resource, Debug)]
pub struct AnnouncerState {
    last: [Option<usize>; LineKey::COUNT],
    timer: Timer,
    fight_announced: bool,
    hold: bool,
}

impl Default for AnnouncerState {
    fn default() -> Self {
        // The timer starts already finished so the empty banner stays hidden
        // until the first announcement resets it.
        let mut timer = Timer::from_seconds(BANNER_SECONDS, TimerMode::Once);
        timer.finish();
        Self {
            last: [None; LineKey::COUNT],
            timer,
            fight_announced: false,
            hold: false,
        }
    }
}

impl AnnouncerState {
    /// Draws the next line of `key` (never the previous one) and fills its
    /// placeholders.
    fn compose(
        &mut self,
        key: LineKey,
        actor: &str,
        opponent: &str,
        value: i32,
        rng: &mut impl Rng,
    ) -> String {
        let pool = lines::pool(key);
        let index = pick_index(pool.len(), self.last[key.index()], rng);
        self.last[key.index()] = Some(index);
        fill_placeholders(pool[index], actor, opponent, value)
    }
}

/// The pool and number carried by one [`CombatEvent`]. The match is
/// exhaustive on purpose: adding a combat event variant without choosing its
/// announcement pool fails the build here.
pub fn event_key(event: CombatEvent) -> (LineKey, i32) {
    match event {
        CombatEvent::Missed | CombatEvent::OutOfReach => (LineKey::Missed, 0),
        CombatEvent::Hit { dmg } => (LineKey::Hit, dmg),
        CombatEvent::Crit { dmg } => (LineKey::Crit, dmg),
        CombatEvent::Blocked { dmg } => (LineKey::Blocked, dmg),
        CombatEvent::Guarded | CombatEvent::Moved { .. } => (LineKey::Guarded, 0),
        CombatEvent::Rested { amount } => (LineKey::Rested, amount),
        CombatEvent::OutOfStamina => (LineKey::OutOfStamina, 0),
        CombatEvent::Defeated => (LineKey::Defeated, 0),
    }
}

/// Substitutes every placeholder a line may carry (see [`lines`] for the
/// vocabulary). `actor` performed the action; `opponent` is the other
/// fighter; `value` is the event's damage or stamina amount.
pub fn fill_placeholders(template: &str, actor: &str, opponent: &str, value: i32) -> String {
    let value = value.to_string();
    let mut line = template.to_string();
    for (placeholder, replacement) in [
        ("{attacker}", actor),
        ("{actor}", actor),
        ("{winner}", actor),
        ("{defender}", opponent),
        ("{loser}", opponent),
        ("{opponent}", opponent),
        ("{dmg}", value.as_str()),
        ("{amount}", value.as_str()),
    ] {
        line = line.replace(placeholder, replacement);
    }
    line
}

/// Picks a uniform index into a pool of `len` lines, never returning `last`
/// again (unless the pool has a single line).
pub fn pick_index(len: usize, last: Option<usize>, rng: &mut impl Rng) -> usize {
    match last {
        // Draw uniformly from the pool minus the previous line: sample one
        // slot short and shift the picks at or past `last` up by one.
        Some(last) if len > 1 && last < len => {
            let index = rng.random_range(0..len - 1);
            if index >= last { index + 1 } else { index }
        }
        _ => rng.random_range(0..len),
    }
}

pub struct AnnouncerPlugin;

impl Plugin for AnnouncerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AnnouncementRequest>()
            .add_systems(OnEnter(GameState::Fight), spawn_banner)
            .add_systems(
                OnExit(GameState::Fight),
                (teardown_announcer, despawn_screen::<AnnouncerBanner>),
            )
            .add_systems(
                Update,
                (announce_fight_start, show_announcements, expire_banner)
                    .chain()
                    .run_if(in_state(GameState::Fight)),
            );
    }
}

/// Spawns the (hidden) banner strip and a fresh [`AnnouncerState`] on
/// entering the fight.
fn spawn_banner(mut commands: Commands, ui_font: Res<UiFont>) {
    commands.insert_resource(AnnouncerState::default());
    commands.spawn((
        AnnouncerBanner,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(BANNER_TOP),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            BannerPanel,
            Node {
                padding: UiRect::axes(Val::Px(18.0), Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(BANNER_BACKGROUND),
            Visibility::Hidden,
            children![(
                BannerText,
                Text::new(""),
                ui_font.text_font(22.0),
                TextColor(CREAM),
            )],
        )],
    ));
}

/// Drops the announcer state on leaving the fight; the banner entities are
/// removed by `despawn_screen::<AnnouncerBanner>`.
fn teardown_announcer(mut commands: Commands) {
    commands.remove_resource::<AnnouncerState>();
}

/// The banner's mutable pieces, bundled so the announcement systems stay
/// under the argument limit and share the show/fade/hide logic.
#[derive(SystemParam)]
struct BannerWidgets<'w, 's> {
    panels:
        Query<'w, 's, (&'static mut Visibility, &'static mut BackgroundColor), With<BannerPanel>>,
    texts: Query<'w, 's, (&'static mut Text, &'static mut TextColor), With<BannerText>>,
}

impl BannerWidgets<'_, '_> {
    /// Shows `line` at full opacity.
    fn show(&mut self, line: &str) {
        for (mut visibility, mut background) in &mut self.panels {
            *visibility = Visibility::Visible;
            background.0 = BANNER_BACKGROUND;
        }
        for (mut text, mut color) in &mut self.texts {
            if text.0 != line {
                text.0 = line.to_string();
            }
            color.0 = CREAM;
        }
    }

    /// Scales the banner's opacity for the fade-out tail.
    fn set_alpha(&mut self, alpha: f32) {
        for (_, mut background) in &mut self.panels {
            background.0 = BANNER_BACKGROUND.with_alpha(BANNER_BACKGROUND.alpha() * alpha);
        }
        for (_, mut color) in &mut self.texts {
            color.0 = CREAM.with_alpha(alpha);
        }
    }

    /// Hides the banner and clears the stale line.
    fn hide(&mut self) {
        for (mut visibility, _) in &mut self.panels {
            *visibility = Visibility::Hidden;
        }
        for (mut text, _) in &mut self.texts {
            if !text.0.is_empty() {
                text.0.clear();
            }
        }
    }
}

/// Query for one side's fighter name.
type NameOf<'w, 's, Side, Other> =
    Query<'w, 's, &'static FighterName, (With<Side>, Without<Other>)>;

/// Query for the enemy's name plus its optional [`Boss`] tag.
type EnemyIntro<'w, 's> = Query<
    'w,
    's,
    (&'static FighterName, Option<&'static Boss>),
    (With<EnemyFighter>, Without<PlayerFighter>),
>;

/// Announces the fight once per fight, as soon as both fighters exist (they
/// are spawned by the arena's own `OnEnter` system). Against a [`Boss`]
/// opponent, the boss's own roster intro line follows in the same frame and
/// wins the banner.
fn announce_fight_start(
    state: Option<ResMut<AnnouncerState>>,
    player: NameOf<PlayerFighter, EnemyFighter>,
    enemy: EnemyIntro,
    mut requests: MessageWriter<AnnouncementRequest>,
) {
    let Some(mut state) = state else {
        return;
    };
    if state.fight_announced {
        return;
    }
    let (Ok(player), Ok((enemy, boss))) = (player.single(), enemy.single()) else {
        return;
    };
    state.fight_announced = true;
    requests.write(AnnouncementRequest {
        key: LineKey::FightStart,
        actor: player.0.clone(),
        opponent: enemy.0.clone(),
        value: 0,
        line: None,
    });
    if let Some(boss) = boss {
        requests.write(AnnouncementRequest {
            key: LineKey::BossIntro,
            actor: player.0.clone(),
            opponent: enemy.0.clone(),
            value: 0,
            line: Some(boss.intro_line.to_string()),
        });
    }
}

/// Turns this frame's [`AnnouncementRequest`]s and [`CombatLogEvent`]s into
/// banner lines; the newest one wins and restarts the banner timer.
fn show_announcements(
    mut requests: MessageReader<AnnouncementRequest>,
    mut events: MessageReader<CombatLogEvent>,
    state: Option<ResMut<AnnouncerState>>,
    rng: Option<ResMut<CombatRng>>,
    player: NameOf<PlayerFighter, EnemyFighter>,
    enemy: NameOf<EnemyFighter, PlayerFighter>,
    mut banner: BannerWidgets,
) {
    let (Some(mut state), Some(mut rng)) = (state, rng) else {
        return;
    };
    let mut line = None;
    for request in requests.read() {
        line = Some(match &request.line {
            Some(template) => {
                fill_placeholders(template, &request.actor, &request.opponent, request.value)
            }
            None => state.compose(
                request.key,
                &request.actor,
                &request.opponent,
                request.value,
                &mut rng.0,
            ),
        });
        // A boss intro holds the banner for its full lifetime: a fast boss
        // acts in the very same frame, and its opening strike line must not
        // eat the intro (the HUD log still records the exchange).
        state.hold = request.key == LineKey::BossIntro;
    }
    let player_name = player.single().map(|n| n.0.as_str()).unwrap_or("?");
    let enemy_name = enemy.single().map(|n| n.0.as_str()).unwrap_or("?");
    for CombatLogEvent { actor, event, .. } in events.read().copied() {
        if state.hold {
            continue;
        }
        let (actor_name, opponent_name) = match actor {
            CombatSide::Player => (player_name, enemy_name),
            CombatSide::Enemy => (enemy_name, player_name),
        };
        let (key, value) = event_key(event);
        line = Some(state.compose(key, actor_name, opponent_name, value, &mut rng.0));
    }
    if let Some(line) = line {
        state.timer.reset();
        banner.show(&line);
    }
}

/// Ages the current banner line: full opacity for most of its life, a fade
/// over the last [`FADE_SECONDS`], then hidden.
fn expire_banner(
    time: Res<Time>,
    state: Option<ResMut<AnnouncerState>>,
    mut banner: BannerWidgets,
) {
    let Some(mut state) = state else {
        return;
    };
    if state.timer.is_finished() {
        return;
    }
    state.timer.tick(time.delta());
    if state.timer.is_finished() {
        state.hold = false;
        banner.hide();
    } else {
        let remaining = state.timer.remaining_secs();
        if remaining < FADE_SECONDS {
            banner.set_alpha((remaining / FADE_SECONDS).clamp(0.0, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaPlugin;
    use crate::character::Attributes;
    use crate::combat::CombatPlugin;
    use crate::core::CorePlugin;
    use crate::creation::PlayerCharacter;
    use crate::flow::FlowPlugin;
    use bevy::state::app::StatesPlugin;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::time::Duration;

    // --- pure pieces ---

    #[test]
    fn every_line_key_has_at_least_five_lines() {
        for key in LineKey::ALL {
            assert!(
                lines::pool(key).len() >= 5,
                "{key:?} needs at least 5 lines, has {}",
                lines::pool(key).len()
            );
        }
    }

    #[test]
    fn every_line_fills_all_its_placeholders() {
        for key in LineKey::ALL {
            for template in lines::pool(key) {
                let line = fill_placeholders(template, "Făt-Frumos", "Strigoi", 42);
                assert!(
                    !line.contains('{') && !line.contains('}'),
                    "{key:?} line left a placeholder: {line}"
                );
            }
        }
    }

    #[test]
    fn placeholders_substitute_names_and_damage_exactly() {
        assert_eq!(
            fill_placeholders(
                "{attacker} lovește ca Sfarmă-Piatră! {dmg} daune!",
                "Făt-Frumos",
                "Strigoi",
                24,
            ),
            "Făt-Frumos lovește ca Sfarmă-Piatră! 24 daune!"
        );
        assert_eq!(
            fill_placeholders(
                "S-a terminat! {loser} pleacă acasă pe jos, prin pădure.",
                "Făt-Frumos",
                "Strigoi",
                0,
            ),
            "S-a terminat! Strigoi pleacă acasă pe jos, prin pădure."
        );
        assert_eq!(
            fill_placeholders("{actor} recuperează {amount} stamina.", "A", "B", 20),
            "A recuperează 20 stamina."
        );
    }

    #[test]
    fn every_combat_event_maps_to_a_pool_and_carries_its_number() {
        let cases = [
            (CombatEvent::Missed, LineKey::Missed, 0),
            (CombatEvent::Hit { dmg: 6 }, LineKey::Hit, 6),
            (CombatEvent::Crit { dmg: 24 }, LineKey::Crit, 24),
            (CombatEvent::Blocked { dmg: 3 }, LineKey::Blocked, 3),
            (CombatEvent::Guarded, LineKey::Guarded, 0),
            (CombatEvent::Rested { amount: 20 }, LineKey::Rested, 20),
            (
                CombatEvent::Moved {
                    from: crate::combat::DuelDistance::FAR,
                    to: crate::combat::DuelDistance::NEAR,
                },
                LineKey::Guarded,
                0,
            ),
            (CombatEvent::OutOfReach, LineKey::Missed, 0),
            (CombatEvent::OutOfStamina, LineKey::OutOfStamina, 0),
            (CombatEvent::Defeated, LineKey::Defeated, 0),
        ];
        for (event, key, value) in cases {
            assert_eq!(event_key(event), (key, value), "{event:?}");
        }
    }

    #[test]
    fn a_hundred_seeded_draws_never_repeat_the_previous_line() {
        for key in LineKey::ALL {
            let len = lines::pool(key).len();
            let mut rng = ChaCha8Rng::seed_from_u64(7);
            let mut last = None;
            for draw in 0..100 {
                let index = pick_index(len, last, &mut rng);
                assert!(index < len, "{key:?} draw {draw} out of range");
                assert_ne!(
                    Some(index),
                    last,
                    "{key:?} repeated line {index} on draw {draw}"
                );
                last = Some(index);
            }
        }
    }

    #[test]
    fn seeded_draws_still_reach_every_other_line() {
        let len = lines::pool(LineKey::Hit).len();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut seen = vec![false; len];
        let mut last = None;
        for _ in 0..100 {
            let index = pick_index(len, last, &mut rng);
            seen[index] = true;
            last = Some(index);
        }
        assert!(seen.iter().all(|&s| s), "all {len} lines drawn: {seen:?}");
    }

    #[test]
    fn a_single_line_pool_repeats_by_necessity() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(pick_index(1, None, &mut rng), 0);
        assert_eq!(pick_index(1, Some(0), &mut rng), 0);
    }

    // --- headless screen behavior ---

    /// Same player build as the combat and HUD tests.
    const PLAYER_ATTRIBUTES: Attributes = Attributes {
        putere: 4,
        agilitate: 2,
        vitalitate: 4,
        noroc: 3,
    };

    /// Headless app on the fight screen with a fixed duel RNG, facing the
    /// first ladder opponent (the Hoț de codru).
    fn test_app() -> App {
        test_app_at(crate::roster::LadderProgress::default(), PLAYER_ATTRIBUTES)
    }

    /// Headless app on the fight screen at `progress` on the ladder. The
    /// player build must not be out-paced by the opponent, or the enemy's
    /// opening action overwrites the fight-start banner in the same frame.
    fn test_app_at(progress: crate::roster::LadderProgress, attributes: Attributes) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, FlowPlugin));
        app.add_plugins((ArenaPlugin, CombatPlugin, AnnouncerPlugin));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_string(),
            attributes,
            appearance: crate::character::PlayerAppearance::default(),
        });
        app.insert_resource(progress);
        app.insert_resource(CombatRng(ChaCha8Rng::seed_from_u64(9)));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        app.update(); // transition + OnEnter + first announcer frame
        app
    }

    fn banner_text(app: &mut App) -> String {
        app.world_mut()
            .query_filtered::<&Text, With<BannerText>>()
            .single(app.world())
            .expect("banner text exists")
            .0
            .clone()
    }

    fn banner_visibility(app: &mut App) -> Visibility {
        *app.world_mut()
            .query_filtered::<&Visibility, With<BannerPanel>>()
            .single(app.world())
            .expect("banner panel exists")
    }

    fn write_event(app: &mut App, actor: CombatSide, event: CombatEvent) {
        app.world_mut().write_message(CombatLogEvent {
            actor,
            action: crate::combat::CombatAction::QuickStrike,
            event,
        });
        app.update();
    }

    /// Ages the current banner line to the end of its life; the next update
    /// ticks the timer over the edge.
    fn expire_current_line(app: &mut App) {
        app.world_mut()
            .resource_mut::<AnnouncerState>()
            .timer
            .set_elapsed(Duration::from_secs_f32(BANNER_SECONDS));
        app.update();
    }

    #[test]
    fn entering_the_fight_announces_both_fighters() {
        let mut app = test_app();
        let text = banner_text(&mut app);
        assert!(
            text.contains("Făt-Frumos") && text.contains("Hoț de codru"),
            "fight-start line names both fighters: {text}"
        );
        assert_eq!(banner_visibility(&mut app), Visibility::Visible);
    }

    #[test]
    fn entering_a_boss_fight_opens_with_the_boss_own_intro_line() {
        // LadderProgress(4) spawns Muma Pădurii, the first boss; the player
        // ties her agilitate 3 so the opening turn stays with the player.
        let mut app = test_app_at(
            crate::roster::LadderProgress(4),
            Attributes {
                agilitate: 3,
                ..PLAYER_ATTRIBUTES
            },
        );
        assert_eq!(
            banner_text(&mut app),
            crate::roster::LADDER[4].intro_line,
            "the boss's roster intro line wins the banner"
        );
        assert_eq!(banner_visibility(&mut app), Visibility::Visible);
    }

    #[test]
    fn the_boss_intro_outlives_the_opening_exchange_then_events_resume() {
        // A fast boss strikes in the same breath as its intro; the intro
        // must hold the banner for its full lifetime anyway.
        let mut app = test_app_at(
            crate::roster::LadderProgress(4),
            Attributes {
                agilitate: 3,
                ..PLAYER_ATTRIBUTES
            },
        );
        let intro = crate::roster::LADDER[4].intro_line;
        write_event(&mut app, CombatSide::Enemy, CombatEvent::Hit { dmg: 9 });
        assert_eq!(
            banner_text(&mut app),
            intro,
            "combat lines never eat a live boss intro"
        );

        expire_current_line(&mut app);
        write_event(&mut app, CombatSide::Player, CombatEvent::Hit { dmg: 6 });
        let text = banner_text(&mut app);
        assert!(
            text.contains("Făt-Frumos") && text.contains('6'),
            "after the intro expires the event lines take over: {text}"
        );
    }

    #[test]
    fn a_combat_event_shows_a_line_with_substituted_name_and_damage() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Hit { dmg: 6 });
        let text = banner_text(&mut app);
        assert!(
            text.contains("Făt-Frumos") && text.contains('6'),
            "hit line names the attacker and the damage: {text}"
        );
        assert_eq!(banner_visibility(&mut app), Visibility::Visible);
    }

    #[test]
    fn an_enemy_event_substitutes_the_enemy_as_the_actor() {
        let mut app = test_app();
        write_event(
            &mut app,
            CombatSide::Enemy,
            CombatEvent::Rested { amount: 20 },
        );
        let text = banner_text(&mut app);
        assert!(
            text.contains("Hoț de codru") && !text.contains("Făt-Frumos"),
            "rest line names the resting enemy, not the player: {text}"
        );
    }

    #[test]
    fn a_defeat_names_the_loser() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Defeated);
        let text = banner_text(&mut app);
        assert!(
            text.contains("Hoț de codru"),
            "the player's victory names the enemy as the loser: {text}"
        );
    }

    #[test]
    fn the_banner_clears_after_its_lifetime() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);
        assert_eq!(banner_visibility(&mut app), Visibility::Visible);
        expire_current_line(&mut app);
        assert_eq!(banner_visibility(&mut app), Visibility::Hidden);
        assert_eq!(banner_text(&mut app), "", "stale line cleared");
    }

    #[test]
    fn a_new_event_replaces_the_line_and_restarts_the_clock() {
        let mut app = test_app();
        write_event(&mut app, CombatSide::Player, CombatEvent::Missed);
        expire_current_line(&mut app);
        assert_eq!(banner_visibility(&mut app), Visibility::Hidden);

        write_event(&mut app, CombatSide::Enemy, CombatEvent::Guarded);
        assert_eq!(
            banner_visibility(&mut app),
            Visibility::Visible,
            "a fresh event revives the banner"
        );
        let text = banner_text(&mut app);
        assert!(
            text.contains("Hoț de codru"),
            "guard line names the actor: {text}"
        );
        assert!(
            !app.world().resource::<AnnouncerState>().timer.is_finished(),
            "the banner clock restarted"
        );
    }

    #[test]
    fn a_boss_intro_request_uses_the_boss_intro_pool() {
        let mut app = test_app();
        app.world_mut().write_message(AnnouncementRequest {
            key: LineKey::BossIntro,
            actor: "Făt-Frumos".to_string(),
            opponent: "Zmeul Zmeilor".to_string(),
            value: 0,
            line: None,
        });
        app.update();
        let text = banner_text(&mut app);
        assert!(
            text.contains("Zmeul Zmeilor"),
            "boss intro names the boss: {text}"
        );
    }

    #[test]
    fn leaving_the_fight_despawns_the_banner_and_drops_the_state() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::FightResult);
        app.update();
        let banners = app
            .world_mut()
            .query_filtered::<(), With<AnnouncerBanner>>()
            .iter(app.world())
            .count();
        assert_eq!(banners, 0, "banner strip despawned");
        assert!(app.world().get_resource::<AnnouncerState>().is_none());
    }
}
