//! Game audio: state-driven folk music and combat / UI sound effects.
//!
//! Music follows [`GameState`]: the menu-side screens share the slow
//! doina-like theme, [`GameState::Fight`] plays the hora-like arena loop
//! (or the ominous boss loop when the current [`LadderProgress`] opponent
//! `is_boss`), and the result screens run silent under their stings. At most
//! one music entity exists at a time: every track change despawns the old
//! [`MusicChannel`] entity before spawning the next.
//!
//! SFX systems are self-contained readers of existing signals: combat sounds
//! come from [`CombatLogEvent`], button clicks from any `Button`'s
//! [`Interaction`] flipping to `Pressed`, and the purchase jingle from the
//! [`OwnedItems`] set growing.
//!
//! On wasm the browser's autoplay policy blocks audio until the user
//! interacts with the page, so music starts only after the first click /
//! key / touch (see [`AudioUnlocked`]); on native it starts immediately.

use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;

use crate::combat::{CombatEvent, CombatLogEvent};
use crate::core::{GameState, despawn_screen};
use crate::menu::{BUTTON_NORMAL, CREAM};
use crate::roster::LadderProgress;
use crate::shop::OwnedItems;

/// Session-scoped volume/mute settings applied to every spawned sound and,
/// live, to every playing sink whenever they change.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AudioSettings {
    /// Linear music volume in `0.0..=1.0`, before mute.
    pub music_volume: f32,
    /// Linear SFX volume in `0.0..=1.0`, before mute.
    pub sfx_volume: f32,
    /// Hard mute: silences music and SFX immediately.
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            music_volume: 0.5,
            sfx_volume: 0.8,
            muted: false,
        }
    }
}

impl AudioSettings {
    /// The music volume actually applied: 0 when muted.
    pub fn effective_music_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.music_volume }
    }

    /// The SFX volume actually applied: 0 when muted.
    pub fn effective_sfx_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.sfx_volume }
    }
}

/// The three music loops shipped under `assets/audio/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicTrack {
    /// Slow doina-like theme for the menu-side screens.
    Menu,
    /// Upbeat hora-like loop for regular fights.
    Arena,
    /// Ominous loop for boss fights.
    Boss,
}

impl MusicTrack {
    /// Asset path of the loop.
    pub fn path(self) -> &'static str {
        match self {
            MusicTrack::Menu => "audio/music_menu.ogg",
            MusicTrack::Arena => "audio/music_arena.ogg",
            MusicTrack::Boss => "audio/music_boss.ogg",
        }
    }
}

/// The track a state calls for; `None` means silence (the result screens
/// play their stings over it). `boss_fight` overrides the arena theme.
pub fn track_for(state: GameState, boss_fight: bool) -> Option<MusicTrack> {
    match state {
        GameState::MainMenu | GameState::CharacterCreation | GameState::Shop => {
            Some(MusicTrack::Menu)
        }
        GameState::Fight => Some(if boss_fight {
            MusicTrack::Boss
        } else {
            MusicTrack::Arena
        }),
        GameState::FightResult | GameState::GameOver | GameState::Victory => None,
    }
}

/// One sound effect; every gameplay/UI trigger maps to exactly one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    /// Normal sword hit.
    Hit,
    /// Critical hit: hit plus a bright ring.
    Crit,
    /// Wood-thunk block (also the guard-raise sound).
    Block,
    /// Whoosh of a missed strike.
    Whoosh,
    /// Recovering breath while resting.
    Rest,
    /// Dull thud of an action rejected for lack of stamina.
    Fail,
    /// Heavy blow that ends the fight.
    Defeated,
    /// UI button click.
    Click,
    /// Coin jingle on a shop purchase.
    Coin,
    /// Short rising sting on victory.
    VictorySting,
    /// Short falling sting on defeat.
    DefeatSting,
}

impl Sfx {
    /// Asset path of the effect.
    pub fn path(self) -> &'static str {
        match self {
            Sfx::Hit => "audio/sfx_hit.ogg",
            Sfx::Crit => "audio/sfx_crit.ogg",
            Sfx::Block => "audio/sfx_block.ogg",
            Sfx::Whoosh => "audio/sfx_whoosh.ogg",
            Sfx::Rest => "audio/sfx_rest.ogg",
            Sfx::Fail => "audio/sfx_fail.ogg",
            Sfx::Defeated => "audio/sfx_defeated.ogg",
            Sfx::Click => "audio/sfx_click.ogg",
            Sfx::Coin => "audio/sfx_coin.ogg",
            Sfx::VictorySting => "audio/sting_victory.ogg",
            Sfx::DefeatSting => "audio/sting_defeat.ogg",
        }
    }
}

/// Total mapping from combat events to sounds: every variant plays something.
pub fn sfx_for(event: CombatEvent) -> Sfx {
    match event {
        CombatEvent::Missed => Sfx::Whoosh,
        CombatEvent::Hit { .. } => Sfx::Hit,
        CombatEvent::Crit { .. } => Sfx::Crit,
        CombatEvent::Blocked { .. } | CombatEvent::Guarded => Sfx::Block,
        CombatEvent::Rested { .. } => Sfx::Rest,
        CombatEvent::OutOfStamina => Sfx::Fail,
        CombatEvent::Defeated => Sfx::Defeated,
    }
}

/// Marker + identity of the single looping music entity.
#[derive(Component, Debug)]
pub struct MusicChannel(pub MusicTrack);

/// Marker for fire-and-forget SFX entities (despawned when done).
#[derive(Component, Debug)]
pub struct SfxChannel;

/// Whether audio may start. `false` on wasm until the first user interaction
/// (browser autoplay policy); always `true` on native.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioUnlocked(pub bool);

impl Default for AudioUnlocked {
    fn default() -> Self {
        Self(!cfg!(target_arch = "wasm32"))
    }
}

/// Marker for the menu's speaker toggle button.
#[derive(Component, Debug)]
struct SpeakerToggle;

/// Marker for the speaker toggle's label text.
#[derive(Component, Debug)]
struct SpeakerToggleLabel;

/// State-driven music plus event-driven SFX. Named `GameAudioPlugin` to
/// avoid clashing with `bevy::audio::AudioPlugin`.
pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        // Registering the message is idempotent; it keeps this plugin
        // self-contained in tests that run it without `CombatPlugin`.
        app.add_message::<CombatLogEvent>()
            .init_resource::<AudioSettings>()
            .init_resource::<AudioUnlocked>()
            .add_systems(OnEnter(GameState::MainMenu), spawn_speaker_toggle)
            .add_systems(OnExit(GameState::MainMenu), despawn_screen::<SpeakerToggle>)
            .add_systems(OnEnter(GameState::FightResult), play_victory_sting)
            .add_systems(OnEnter(GameState::Victory), play_victory_sting)
            .add_systems(OnEnter(GameState::GameOver), play_defeat_sting)
            .add_systems(
                Update,
                (
                    unlock_audio_on_interaction,
                    toggle_mute_on_key,
                    handle_speaker_toggle,
                    sync_music,
                    combat_sfx,
                    ui_click_sfx,
                    purchase_sfx,
                    apply_settings_to_sinks,
                    update_speaker_label,
                )
                    .chain(),
            );
    }
}

/// Flips [`AudioUnlocked`] on the first click, key press, or touch. The
/// browser resumes the audio context on the same interaction, so starting
/// music afterwards never trips the autoplay policy.
fn unlock_audio_on_interaction(
    mut unlocked: ResMut<AudioUnlocked>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    touches: Res<Touches>,
) {
    if unlocked.0 {
        return;
    }
    if mouse.get_just_pressed().next().is_some()
        || keys.get_just_pressed().next().is_some()
        || touches.any_just_pressed()
    {
        unlocked.0 = true;
    }
}

/// M toggles the global mute.
fn toggle_mute_on_key(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<AudioSettings>) {
    if keys.just_pressed(KeyCode::KeyM) {
        settings.muted = !settings.muted;
    }
}

/// Keeps the single music entity in sync with the current state: despawns
/// the old loop and spawns the desired one whenever the wanted track
/// changes. Runs every frame but only acts on an actual change.
fn sync_music(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    state: Res<State<GameState>>,
    ladder: Option<Res<LadderProgress>>,
    settings: Res<AudioSettings>,
    unlocked: Res<AudioUnlocked>,
    playing: Query<(Entity, &MusicChannel)>,
) {
    let desired = if unlocked.0 {
        let boss_fight = ladder.map(|l| l.opponent().is_boss).unwrap_or(false);
        track_for(*state.get(), boss_fight)
    } else {
        None
    };
    let current = playing.iter().next().map(|(_, channel)| channel.0);
    if desired == current {
        return;
    }
    for (entity, _) in &playing {
        commands.entity(entity).despawn();
    }
    if let Some(track) = desired {
        commands.spawn((
            AudioPlayer::new(asset_server.load(track.path())),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                volume: Volume::Linear(settings.effective_music_volume()),
                ..default()
            },
            MusicChannel(track),
        ));
    }
}

/// Spawns a fire-and-forget SFX entity honoring the current settings.
fn spawn_sfx(
    commands: &mut Commands,
    asset_server: &AssetServer,
    settings: &AudioSettings,
    sfx: Sfx,
) {
    commands.spawn((
        AudioPlayer::new(asset_server.load(sfx.path())),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(settings.effective_sfx_volume()),
            ..default()
        },
        SfxChannel,
    ));
}

/// Plays the mapped sound for every combat event, whoever acted.
fn combat_sfx(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    settings: Res<AudioSettings>,
    mut events: MessageReader<CombatLogEvent>,
) {
    for event in events.read() {
        spawn_sfx(
            &mut commands,
            &asset_server,
            &settings,
            sfx_for(event.event),
        );
    }
}

/// Clicks for every UI button press, across all screens.
fn ui_click_sfx(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    settings: Res<AudioSettings>,
    interactions: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            spawn_sfx(&mut commands, &asset_server, &settings, Sfx::Click);
        }
    }
}

/// Coin jingle whenever the owned-items set grows (i.e. a purchase landed);
/// equips and refused buys leave the set unchanged and stay silent. Only
/// jingles inside the shop — the set can also grow when "Continuă" loads a
/// save on the menu, which must stay silent; the baseline count is kept in
/// sync every frame so such jumps never fire a late sound.
fn purchase_sfx(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    settings: Res<AudioSettings>,
    state: Res<State<GameState>>,
    owned: Option<Res<OwnedItems>>,
    mut last_count: Local<usize>,
) {
    let Some(owned) = owned else {
        return;
    };
    let count = owned.0.len();
    if *state.get() == GameState::Shop
        && owned.is_changed()
        && count > *last_count
        && !owned.is_added()
    {
        spawn_sfx(&mut commands, &asset_server, &settings, Sfx::Coin);
    }
    *last_count = count;
}

fn play_victory_sting(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    settings: Res<AudioSettings>,
) {
    spawn_sfx(&mut commands, &asset_server, &settings, Sfx::VictorySting);
}

fn play_defeat_sting(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    settings: Res<AudioSettings>,
) {
    spawn_sfx(&mut commands, &asset_server, &settings, Sfx::DefeatSting);
}

/// Pushes changed settings into every live sink so mute/volume take effect
/// immediately instead of only on the next spawned sound.
fn apply_settings_to_sinks(
    settings: Res<AudioSettings>,
    mut music: Query<&mut AudioSink, (With<MusicChannel>, Without<SfxChannel>)>,
    mut sfx: Query<&mut AudioSink, (With<SfxChannel>, Without<MusicChannel>)>,
) {
    if !settings.is_changed() {
        return;
    }
    for mut sink in &mut music {
        sink.set_volume(Volume::Linear(settings.effective_music_volume()));
    }
    for mut sink in &mut sfx {
        sink.set_volume(Volume::Linear(settings.effective_sfx_volume()));
    }
}

/// The speaker toggle's label for the current mute state.
fn speaker_label(muted: bool) -> &'static str {
    if muted {
        "Sunet: oprit (M)"
    } else {
        "Sunet: pornit (M)"
    }
}

/// Small speaker toggle pinned to the menu's top-right corner.
fn spawn_speaker_toggle(mut commands: Commands, settings: Res<AudioSettings>) {
    commands
        .spawn((
            SpeakerToggle,
            Button,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(16.0),
                right: Val::Px(16.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(BUTTON_NORMAL),
        ))
        .with_children(|parent| {
            parent.spawn((
                SpeakerToggleLabel,
                Text::new(speaker_label(settings.muted)),
                TextFont::from_font_size(16.0),
                TextColor(CREAM),
            ));
        });
}

/// Flips mute when the speaker toggle is pressed.
fn handle_speaker_toggle(
    interactions: Query<&Interaction, (Changed<Interaction>, With<SpeakerToggle>)>,
    mut settings: ResMut<AudioSettings>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            settings.muted = !settings.muted;
        }
    }
}

/// Keeps the speaker label in sync with the mute state (M key included).
fn update_speaker_label(
    settings: Res<AudioSettings>,
    mut labels: Query<&mut Text, With<SpeakerToggleLabel>>,
) {
    if !settings.is_changed() {
        return;
    }
    for mut text in &mut labels {
        text.0 = speaker_label(settings.muted).to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::audio::AudioPlugin;
    use bevy::state::app::StatesPlugin;

    // ── pure mapping tests ──────────────────────────────────────────────

    #[test]
    fn menu_side_states_play_the_menu_theme() {
        for state in [
            GameState::MainMenu,
            GameState::CharacterCreation,
            GameState::Shop,
        ] {
            assert_eq!(track_for(state, false), Some(MusicTrack::Menu));
            // A stale boss flag must not leak outside the fight.
            assert_eq!(track_for(state, true), Some(MusicTrack::Menu));
        }
    }

    #[test]
    fn fight_plays_arena_theme_and_boss_overrides_it() {
        assert_eq!(track_for(GameState::Fight, false), Some(MusicTrack::Arena));
        assert_eq!(track_for(GameState::Fight, true), Some(MusicTrack::Boss));
    }

    #[test]
    fn result_screens_are_silent() {
        for boss in [false, true] {
            assert_eq!(track_for(GameState::FightResult, boss), None);
            assert_eq!(track_for(GameState::GameOver, boss), None);
            assert_eq!(track_for(GameState::Victory, boss), None);
        }
    }

    #[test]
    fn every_combat_event_maps_to_an_sfx() {
        // Exhaustive by construction: `sfx_for` matches without a wildcard,
        // so a new variant breaks the build. This pins the current mapping.
        let cases = [
            (CombatEvent::Missed, Sfx::Whoosh),
            (CombatEvent::Hit { dmg: 3 }, Sfx::Hit),
            (CombatEvent::Crit { dmg: 6 }, Sfx::Crit),
            (CombatEvent::Blocked { dmg: 1 }, Sfx::Block),
            (CombatEvent::Guarded, Sfx::Block),
            (CombatEvent::Rested { amount: 2 }, Sfx::Rest),
            (CombatEvent::OutOfStamina, Sfx::Fail),
            (CombatEvent::Defeated, Sfx::Defeated),
        ];
        for (event, expected) in cases {
            assert_eq!(sfx_for(event), expected, "{event:?}");
        }
    }

    #[test]
    fn every_audio_asset_path_exists_on_disk() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let tracks = [MusicTrack::Menu, MusicTrack::Arena, MusicTrack::Boss];
        let sfx = [
            Sfx::Hit,
            Sfx::Crit,
            Sfx::Block,
            Sfx::Whoosh,
            Sfx::Rest,
            Sfx::Fail,
            Sfx::Defeated,
            Sfx::Click,
            Sfx::Coin,
            Sfx::VictorySting,
            Sfx::DefeatSting,
        ];
        for path in tracks
            .iter()
            .map(|t| t.path())
            .chain(sfx.iter().map(|s| s.path()))
        {
            assert!(assets.join(path).is_file(), "missing asset: {path}");
        }
    }

    #[test]
    fn mute_zeroes_effective_volumes_and_unmute_restores_them() {
        let mut settings = AudioSettings {
            music_volume: 0.5,
            sfx_volume: 0.8,
            muted: false,
        };
        assert_eq!(settings.effective_music_volume(), 0.5);
        assert_eq!(settings.effective_sfx_volume(), 0.8);
        settings.muted = true;
        assert_eq!(settings.effective_music_volume(), 0.0);
        assert_eq!(settings.effective_sfx_volume(), 0.0);
        settings.muted = false;
        assert_eq!(settings.effective_music_volume(), 0.5);
    }

    // ── app-level tests ─────────────────────────────────────────────────

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            AudioPlugin::default(),
            StatesPlugin,
            crate::core::CorePlugin,
            GameAudioPlugin,
        ));
        // Bare input resources instead of `InputPlugin`, so presses injected
        // by tests are not cleared by the input frame systems.
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<Touches>();
        // Native default is unlocked; force it for determinism.
        app.insert_resource(AudioUnlocked(true));
        app.update();
        app
    }

    fn set_state(app: &mut App, state: GameState) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(state);
        app.update();
        app.update();
    }

    fn playing_tracks(app: &mut App) -> Vec<MusicTrack> {
        app.world_mut()
            .query::<&MusicChannel>()
            .iter(app.world())
            .map(|channel| channel.0)
            .collect()
    }

    #[test]
    fn exactly_one_music_entity_follows_state_transitions() {
        let mut app = test_app();
        assert_eq!(playing_tracks(&mut app), vec![MusicTrack::Menu]);

        app.insert_resource(LadderProgress(0)); // opponent 0 is not a boss
        set_state(&mut app, GameState::Fight);
        assert_eq!(playing_tracks(&mut app), vec![MusicTrack::Arena]);

        set_state(&mut app, GameState::FightResult);
        assert_eq!(playing_tracks(&mut app), vec![]);

        set_state(&mut app, GameState::Shop);
        assert_eq!(playing_tracks(&mut app), vec![MusicTrack::Menu]);
    }

    #[test]
    fn boss_opponent_swaps_in_the_boss_theme() {
        let mut app = test_app();
        let boss_index = crate::roster::LADDER
            .iter()
            .position(|o| o.is_boss)
            .expect("ladder has a boss");
        app.insert_resource(LadderProgress(boss_index));
        set_state(&mut app, GameState::Fight);
        assert_eq!(playing_tracks(&mut app), vec![MusicTrack::Boss]);
    }

    #[test]
    fn m_key_toggles_mute() {
        let mut app = test_app();
        assert!(!app.world().resource::<AudioSettings>().muted);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyM);
        app.update();
        assert!(app.world().resource::<AudioSettings>().muted);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear_just_pressed(KeyCode::KeyM);
        keys.release(KeyCode::KeyM);
        keys.clear_just_released(KeyCode::KeyM);
        keys.press(KeyCode::KeyM);
        app.update();
        assert!(!app.world().resource::<AudioSettings>().muted);
    }

    #[test]
    fn locked_audio_spawns_no_music_until_an_interaction() {
        let mut app = test_app();
        app.insert_resource(AudioUnlocked(false));
        // Drain the already-playing menu loop caused by the unlocked start.
        app.update();
        assert_eq!(playing_tracks(&mut app), vec![]);

        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        app.update();
        assert!(app.world().resource::<AudioUnlocked>().0);
        assert_eq!(playing_tracks(&mut app), vec![MusicTrack::Menu]);
    }

    #[test]
    fn combat_events_spawn_sfx_entities() {
        let mut app = test_app();
        app.world_mut().write_message(CombatLogEvent {
            actor: crate::combat::CombatSide::Player,
            event: CombatEvent::Hit { dmg: 4 },
        });
        app.update();
        let count = app
            .world_mut()
            .query::<&SfxChannel>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn growing_owned_items_plays_the_coin_sfx_but_initial_load_does_not() {
        use crate::items::ItemId;
        use std::collections::HashSet;

        let mut app = test_app();
        let sfx_count = |app: &mut App| {
            app.world_mut()
                .query::<&SfxChannel>()
                .iter(app.world())
                .count()
        };

        // A set loaded from a save on the menu must not jingle.
        app.insert_resource(OwnedItems(HashSet::from([ItemId::Palos])));
        app.update();
        app.update();
        assert_eq!(sfx_count(&mut app), 0);

        // A purchase inside the shop does.
        set_state(&mut app, GameState::Shop);
        app.world_mut()
            .resource_mut::<OwnedItems>()
            .0
            .insert(ItemId::CaciulaDeOaie);
        app.update();
        assert_eq!(sfx_count(&mut app), 1);
    }

    #[test]
    fn speaker_toggle_spawns_on_menu_and_flips_mute() {
        let mut app = test_app();
        let button = app
            .world_mut()
            .query_filtered::<Entity, With<SpeakerToggle>>()
            .iter(app.world())
            .next()
            .expect("menu spawns the speaker toggle");
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        assert!(app.world().resource::<AudioSettings>().muted);
    }
}
