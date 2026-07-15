//! Settings overlay (#30, #191): music / SFX volume steppers, a master mute
//! toggle, and the reduced-motion / high-contrast accessibility toggles,
//! opened over the main menu (its **Setări** button) or over the pause
//! overlay mid-fight (its **Setări** button). It is an overlay, not a
//! `GameState`: opening it never transitions state, so a paused fight
//! stays exactly as it was and **Înapoi** simply despawns the panel.
//!
//! Volumes are discrete steps `0..=10` mapped linearly onto the
//! [`AudioSettings`] `0.0..=1.0` volumes; changes hit the resource directly,
//! and the audio plugin's sink-sync system applies them to the playing track
//! immediately. The two accessibility toggles (#191) work the same way
//! against [`AccessibilityPreferences`]: this module only persists the
//! *preference* and exposes the resource — it deliberately does not act on
//! it. Later systems (#200 reduced-motion suppression, #214 high-contrast
//! tokens) read `Res<AccessibilityPreferences>` and change their own
//! behavior; nothing here changes game presentation.
//!
//! Every change is persisted under its own key ([`SETTINGS_KEY`]) via the
//! #21 storage backends, separate from the run save — game over deletes the
//! run, never the settings (see [`SettingsStore`] and #191's added test
//! coverage for that separation).
//!
//! ## Versioning and migration (#191)
//!
//! [`SETTINGS_VERSION`] moved from `1` (audio-only: music/sfx/muted) to `2`
//! (adds `reduced_motion`/`high_contrast`), but [`SETTINGS_KEY`] is
//! unchanged — the same stored blob is upgraded in place, never relocated.
//! [`SettingsSave::from_json`] reads the blob's own `version` field first and
//! dispatches: a current-version (`2`) blob deserializes directly; a `1`
//! blob upgrades through [`SettingsSaveV1`], keeping every audio value
//! byte-for-byte and defaulting both new accessibility fields to `false`;
//! any other version (corrupt JSON, a future version this build doesn't
//! know about, or a missing/garbled `version` field) yields `None`, so the
//! documented defaults apply — never a panic.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::AudioSettings;
use crate::core::UiFont;
use crate::save::{SaveBackend, platform_backend};
use crate::theme::{
    BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, PanelTexture, SCRIM_HEAVY, panel_bundle,
};
use crate::ui_widgets::focus::{
    FocusNavigationPlugin, FocusNavigationSet, InputFocus, PendingAutofocus, TabGroup,
    TabNavigation, autofocus_first_in_group,
};
use crate::ui_widgets::{attribute_row::spawn_stepper_row, wide_button, wide_button_labeled};

/// The version written into every stored settings blob. Version `1`
/// (audio-only) safely migrates to this version (see [`SettingsSaveV1`] and
/// [`SettingsSave::from_json`]); any other version is discarded and the
/// defaults apply.
pub const SETTINGS_VERSION: u32 = 2;

/// The prior, audio-only settings version (#30), kept around only as the one
/// migration source [`SettingsSave::from_json`] upgrades from.
const SETTINGS_VERSION_V1_AUDIO_ONLY: u32 = 1;

/// The settings' own storage key (`localStorage` on wasm); native stores the
/// blob in `settings.json` next to the run save.
pub const SETTINGS_KEY: &str = "rff_settings_v1";

/// The native settings file name under the game's data directory.
pub const SETTINGS_FILE: &str = "settings.json";

/// Number of volume steps: volumes run `0..=VOLUME_STEPS`.
pub const VOLUME_STEPS: u32 = 10;

/// A volume step `0..=10` as the linear `0.0..=1.0` volume.
pub fn step_to_volume(step: u32) -> f32 {
    step.min(VOLUME_STEPS) as f32 / VOLUME_STEPS as f32
}

/// A linear volume back to its nearest step, clamped into `0..=10`.
pub fn volume_to_step(volume: f32) -> u32 {
    ((volume.clamp(0.0, 1.0) * VOLUME_STEPS as f32).round()) as u32
}

/// Marker resource: present exactly while the settings overlay is open.
/// Inserted by the main menu's and the pause overlay's **Setări** buttons;
/// removed by **Înapoi**.
#[derive(Resource, Debug)]
pub struct SettingsOpen;

/// Runtime accessibility preferences (#191): whether the player asked for
/// reduced motion and/or high contrast. This module owns only the
/// preference itself — persisting it and exposing it here as a plain
/// `Resource` for later systems to *observe*. It never suppresses motion or
/// re-themes anything; #200 (motion suppression) and #214 (contrast tokens)
/// are the systems that read `Res<AccessibilityPreferences>` and change
/// their own behavior accordingly.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccessibilityPreferences {
    /// The player asked for reduced motion (safe default: `false`).
    pub reduced_motion: bool,
    /// The player asked for high contrast (safe default: `false`).
    pub high_contrast: bool,
}

/// Serde snapshot of [`AudioSettings`] and [`AccessibilityPreferences`],
/// stored under [`SETTINGS_KEY`].
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct SettingsSave {
    /// Always [`SETTINGS_VERSION`]; any other value (except the migratable
    /// [`SETTINGS_VERSION_V1_AUDIO_ONLY`]) discards the blob.
    pub version: u32,
    /// Music volume step `0..=10`.
    pub music: u32,
    /// SFX volume step `0..=10`.
    pub sfx: u32,
    /// Master mute.
    pub muted: bool,
    /// Reduced-motion preference (#191); defaults to `false` on a v1 blob
    /// that predates it.
    #[serde(default)]
    pub reduced_motion: bool,
    /// High-contrast preference (#191); defaults to `false` on a v1 blob
    /// that predates it.
    #[serde(default)]
    pub high_contrast: bool,
}

/// The audio-only settings blob written by #30, before #191 added the
/// accessibility fields. [`SettingsSave::from_json`] is the only reader of
/// this shape, upgrading it into a current [`SettingsSave`] with both new
/// fields defaulted to `false` and every audio value carried over intact.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
struct SettingsSaveV1 {
    version: u32,
    music: u32,
    sfx: u32,
    muted: bool,
}

impl SettingsSave {
    /// Snapshots the live [`AudioSettings`] and [`AccessibilityPreferences`].
    pub fn capture(audio: &AudioSettings, accessibility: &AccessibilityPreferences) -> Self {
        Self {
            version: SETTINGS_VERSION,
            music: volume_to_step(audio.music_volume),
            sfx: volume_to_step(audio.sfx_volume),
            muted: audio.muted,
            reduced_motion: accessibility.reduced_motion,
            high_contrast: accessibility.high_contrast,
        }
    }

    /// The snapshot back as live [`AudioSettings`]; out-of-range steps clamp.
    pub fn audio_settings(&self) -> AudioSettings {
        AudioSettings {
            music_volume: step_to_volume(self.music),
            sfx_volume: step_to_volume(self.sfx),
            muted: self.muted,
        }
    }

    /// The snapshot back as live [`AccessibilityPreferences`].
    pub fn accessibility_preferences(&self) -> AccessibilityPreferences {
        AccessibilityPreferences {
            reduced_motion: self.reduced_motion,
            high_contrast: self.high_contrast,
        }
    }

    /// The snapshot as JSON; `None` only if serialization itself fails.
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    /// Parses and validates a stored blob: a current-version ([`SETTINGS_VERSION`])
    /// blob deserializes directly; a [`SETTINGS_VERSION_V1_AUDIO_ONLY`] blob
    /// migrates (audio values kept, accessibility fields default to `false`);
    /// anything else — corrupt JSON, a missing/non-numeric `version`, or any
    /// other version number (including a future one this build doesn't know
    /// about) — yields `None` so the documented defaults apply. Never
    /// panics.
    pub fn from_json(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let version = value.get("version")?.as_u64()?;
        match u32::try_from(version) {
            Ok(SETTINGS_VERSION) => serde_json::from_value(value).ok(),
            Ok(SETTINGS_VERSION_V1_AUDIO_ONLY) => {
                let v1: SettingsSaveV1 = serde_json::from_value(value).ok()?;
                Some(Self {
                    version: SETTINGS_VERSION,
                    music: v1.music,
                    sfx: v1.sfx,
                    muted: v1.muted,
                    reduced_motion: false,
                    high_contrast: false,
                })
            }
            Ok(other) => {
                warn!(
                    "settings version {other} is not the current version ({SETTINGS_VERSION}) or a migratable one ({SETTINGS_VERSION_V1_AUDIO_ONLY}); using defaults"
                );
                None
            }
            Err(_) => None,
        }
    }
}

/// Where the settings blob lives: the platform backend under
/// [`SETTINGS_KEY`] / [`SETTINGS_FILE`] by default, an in-memory one in
/// tests. Deliberately a separate resource from the run's `SaveStore` so
/// game-over save deletion can never touch the settings.
#[derive(Resource)]
pub struct SettingsStore(Box<dyn SaveBackend>);

impl SettingsStore {
    /// A store over a specific backend (tests use the in-memory one).
    pub fn with_backend(backend: impl SaveBackend) -> Self {
        Self(Box::new(backend))
    }

    /// Writes the settings blob, replacing any previous one.
    pub fn store(&self, json: &str) {
        self.0.store(json);
    }

    /// The stored settings blob, if any.
    pub fn load(&self) -> Option<String> {
        self.0.load()
    }
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self(Box::new(platform_backend(SETTINGS_FILE, SETTINGS_KEY)))
    }
}

/// Marker for the settings-overlay root (the semi-transparent scrim).
#[derive(Component)]
struct SettingsOverlay;

/// Marker for the settings panel nested inside [`SettingsOverlay`] -- the
/// actual `TabGroup::modal()` root #216's [`autofocus_settings_overlay`]
/// targets, and the entity `panel_bundle`'s own `Node` (not the full-window
/// scrim) sizes to its content.
#[derive(Component)]
struct SettingsPanel;

/// What a settings-overlay button does when clicked.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// Music `-` / `+`.
    MusicStep(i32),
    /// SFX `-` / `+`.
    SfxStep(i32),
    /// «Sunet: Pornit/Oprit» master mute toggle.
    ToggleMute,
    /// «Mișcare redusă: Pornit/Oprit» reduced-motion preference toggle (#191).
    ToggleReducedMotion,
    /// «Contrast ridicat: Pornit/Oprit» high-contrast preference toggle (#191).
    ToggleHighContrast,
    /// «Înapoi» — close the overlay, back to wherever it was opened from.
    Back,
}

/// Which live value a text label shows; `update_labels` refreshes every
/// carrier whenever [`AudioSettings`] or [`AccessibilityPreferences`]
/// changes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsLabel {
    /// The music volume step `0..=10`.
    Music,
    /// The SFX volume step `0..=10`.
    Sfx,
    /// The mute toggle's «Sunet: Pornit/Oprit» text.
    Mute,
    /// The reduced-motion toggle's «Mișcare redusă: Pornit/Oprit» text (#191).
    ReducedMotion,
    /// The high-contrast toggle's «Contrast ridicat: Pornit/Oprit» text (#191).
    HighContrast,
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SettingsStore>()
            .init_resource::<AccessibilityPreferences>()
            .add_plugins((crate::ui_widgets::ScrollInputPlugin, FocusNavigationPlugin))
            .add_systems(Startup, load_settings)
            .add_systems(
                Update,
                (
                    spawn_overlay.run_if(resource_added::<SettingsOpen>),
                    autofocus_settings_overlay.run_if(resource_added::<SettingsOpen>),
                    handle_settings_buttons
                        .after(FocusNavigationSet)
                        .run_if(resource_exists::<SettingsOpen>),
                    update_button_backgrounds.run_if(resource_exists::<SettingsOpen>),
                    update_labels.run_if(resource_exists::<SettingsOpen>),
                    // #216: the overlay's scrim scrolls on short viewports
                    // (see `spawn_overlay`); this drives it for wheel/touch,
                    // scoped to the overlay being open (each screen runs its
                    // own instance for its own scrollables, per #31).
                    crate::ui_widgets::scroll_with_wheel_and_touch
                        .run_if(resource_exists::<SettingsOpen>),
                    despawn_overlay.run_if(resource_removed::<SettingsOpen>),
                    persist_on_change,
                )
                    .chain(),
            );
    }
}

/// Applies the stored settings to [`AudioSettings`] and
/// [`AccessibilityPreferences`] at startup. A missing, corrupt, or
/// unmigratable-version blob leaves the defaults in place for both.
fn load_settings(
    store: Res<SettingsStore>,
    mut audio: ResMut<AudioSettings>,
    mut accessibility: ResMut<AccessibilityPreferences>,
) {
    let Some(save) = store.load().as_deref().and_then(SettingsSave::from_json) else {
        return;
    };
    *audio = save.audio_settings();
    *accessibility = save.accessibility_preferences();
}

/// Persists [`AudioSettings`] and [`AccessibilityPreferences`] whenever
/// either changes (steppers, the mute toggle, the M key, or the two
/// accessibility toggles alike). The startup tick is skipped for each: the
/// values `load_settings` just applied must not immediately rewrite the
/// store.
fn persist_on_change(
    audio: Res<AudioSettings>,
    accessibility: Res<AccessibilityPreferences>,
    store: Res<SettingsStore>,
) {
    let audio_dirty = audio.is_changed() && !audio.is_added();
    let accessibility_dirty = accessibility.is_changed() && !accessibility.is_added();
    if !audio_dirty && !accessibility_dirty {
        return;
    }
    match SettingsSave::capture(&audio, &accessibility).to_json() {
        Some(json) => store.store(&json),
        None => warn!("could not serialize the settings; nothing stored"),
    }
}

/// The mute toggle's label for the current state.
fn mute_label(muted: bool) -> &'static str {
    if muted {
        "Sunet: Oprit"
    } else {
        "Sunet: Pornit"
    }
}

/// The reduced-motion toggle's label for the current state (#191).
fn reduced_motion_label(enabled: bool) -> &'static str {
    if enabled {
        "Mișcare redusă: Pornit"
    } else {
        "Mișcare redusă: Oprit"
    }
}

/// The high-contrast toggle's label for the current state (#191).
fn high_contrast_label(enabled: bool) -> &'static str {
    if enabled {
        "Contrast ridicat: Pornit"
    } else {
        "Contrast ridicat: Oprit"
    }
}

/// Spawns the settings scrim and panel above everything else (the pause
/// overlay sits at `GlobalZIndex(10)`).
fn spawn_overlay(
    mut commands: Commands,
    audio: Res<AudioSettings>,
    accessibility: Res<AccessibilityPreferences>,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
) {
    commands
        .spawn((
            SettingsOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                // #216: on a short viewport (200% desktop zoom halves the
                // CSS-pixel height) the panel is taller than the window --
                // the scrim scrolls it into reach (#31's `Scrollable`
                // pattern), via wheel/touch for pointer users and via the
                // shared focus widget's scroll-into-view for keyboard users.
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(SCRIM_HEAVY),
            GlobalZIndex(20),
            ScrollPosition::default(),
            crate::ui_widgets::Scrollable,
        ))
        .with_children(|parent| {
            parent
                .spawn((
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
                    SettingsPanel,
                    // #216: a *modal* group -- once focus is inside it, Tab
                    // must never reach whatever screen the overlay sits
                    // over (main menu or the paused fight). See
                    // `crate::ui_widgets::focus`'s registration API and
                    // `autofocus_settings_overlay` for why entry into this
                    // group has to be set explicitly.
                    TabGroup::modal(),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Setări"),
                        ui_font.text_font(34.0),
                        TextColor(CREAM),
                    ));
                    spawn_stepper_row(
                        panel,
                        "Muzică",
                        volume_to_step(audio.music_volume),
                        SettingsAction::MusicStep(-1),
                        SettingsAction::MusicStep(1),
                        SettingsLabel::Music,
                        &ui_font,
                    );
                    spawn_stepper_row(
                        panel,
                        "Efecte",
                        volume_to_step(audio.sfx_volume),
                        SettingsAction::SfxStep(-1),
                        SettingsAction::SfxStep(1),
                        SettingsLabel::Sfx,
                        &ui_font,
                    );
                    panel.spawn((
                        wide_button_labeled(mute_label(audio.muted), SettingsLabel::Mute, &ui_font),
                        SettingsAction::ToggleMute,
                    ));
                    panel.spawn((
                        wide_button_labeled(
                            reduced_motion_label(accessibility.reduced_motion),
                            SettingsLabel::ReducedMotion,
                            &ui_font,
                        ),
                        SettingsAction::ToggleReducedMotion,
                    ));
                    panel.spawn((
                        wide_button_labeled(
                            high_contrast_label(accessibility.high_contrast),
                            SettingsLabel::HighContrast,
                            &ui_font,
                        ),
                        SettingsAction::ToggleHighContrast,
                    ));
                    panel.spawn((wide_button("Înapoi", &ui_font), SettingsAction::Back));
                });
        });
}

/// Query filter: settings buttons whose interaction changed this frame.
type ChangedSettingsButton = (Changed<Interaction>, With<Button>);

/// Query filter: [`SettingsAction`] carriers whose interaction changed this
/// frame (for the hover/pressed feedback).
type ChangedSettingsActionButton = (Changed<Interaction>, With<SettingsAction>);

/// Hover/pressed feedback for the overlay's buttons (the same pattern as the
/// menu and the pause overlay). The menu's own feedback system only runs in
/// `MainMenu`, so the overlay brings its own for the paused-fight entry
/// point; scoping it to [`SettingsAction`] keeps the two from fighting.
fn update_button_backgrounds(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedSettingsActionButton>,
) {
    for (interaction, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None => BUTTON_NORMAL,
        };
    }
}

/// Removes the overlay tree when [`SettingsOpen`] is removed, then either
/// re-focuses another still-open modal overlay or clears [`InputFocus`]
/// (#216).
///
/// The overlay's `TabGroup::modal()` confines focus to its own controls
/// while open (see `spawn_overlay`'s doc comment), so whatever `InputFocus`
/// names here is necessarily one of the entities just despawned. This
/// overlay opens from *two* places — the main menu's **Setări** and the
/// paused fight's own **Setări** — and in the second case the pause
/// overlay's own `TabGroup::modal()` is still alive underneath. Plain
/// clearing would leave the next Tab press to fall through to whatever
/// non-modal group the *fight screen itself* has (its HUD, its action
/// palette): `TabNavigation::navigate`'s modal handling only takes effect
/// once focus is already inside a modal group (see
/// [`autofocus_first_in_group`]'s doc comment), so re-entering the pause
/// overlay after this one closes has to be explicit, the same as opening it
/// was. Detecting "is there another modal overlay still open" generically
/// (any other live `TabGroup::modal()`) keeps this overlay decoupled from
/// knowing about `combat::pause` by name.
fn despawn_overlay(
    mut commands: Commands,
    overlays: Query<Entity, With<SettingsOverlay>>,
    nav: TabNavigation,
    mut focus: Option<ResMut<InputFocus>>,
    mut pending: ResMut<PendingAutofocus>,
    other_modals: Query<(Entity, &TabGroup), Without<SettingsPanel>>,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let Some(focus) = focus.as_mut() else {
        return;
    };
    match other_modals.iter().find(|(_, tg)| tg.modal) {
        // #268: `other_modal`'s own `Focusable` children can, in principle,
        // be mid-spawn on the exact frame this runs (the same slow-first-
        // frame race documented on `autofocus_pause_overlay`) -- see
        // `autofocus_first_in_group`'s doc comment on `PendingAutofocus`
        // retrying this instead of leaving focus stuck nowhere.
        Some((other_modal, _)) => {
            autofocus_first_in_group(&nav, focus, &mut pending, other_modal);
        }
        None => focus.clear(),
    }
}

/// Focuses the overlay's first control the instant it spawns (#216): see
/// [`autofocus_first_in_group`]'s doc comment for why a modal group needs
/// this rather than relying on `TabNavigation`'s own modal handling. Ordered
/// after `spawn_overlay` in [`SettingsPlugin`]'s chained `Update` tuple,
/// which — like every `.chain()`d tuple — applies deferred `Commands`
/// between each pair of systems, so the panel this queries for already
/// exists by the time this runs, in the same frame [`SettingsOpen`] was
/// inserted.
fn autofocus_settings_overlay(
    nav: TabNavigation,
    mut focus: ResMut<InputFocus>,
    mut pending: ResMut<PendingAutofocus>,
    panels: Query<Entity, With<SettingsPanel>>,
) {
    for panel in &panels {
        autofocus_first_in_group(&nav, &mut focus, &mut pending, panel);
    }
}

/// Applies a clicked settings action: step a volume (clamped at 0/10), flip
/// the mute or an accessibility toggle, or close the overlay. Audio changes
/// go through [`AudioSettings`], which the audio plugin applies to live
/// sinks the same frame; the two accessibility toggles only flip
/// [`AccessibilityPreferences`] — nothing here reacts to them (see the
/// module docs).
fn handle_settings_buttons(
    mut commands: Commands,
    interactions: Query<(&Interaction, &SettingsAction), ChangedSettingsButton>,
    mut audio: ResMut<AudioSettings>,
    mut accessibility: ResMut<AccessibilityPreferences>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            SettingsAction::MusicStep(delta) => {
                let step = volume_to_step(audio.music_volume).saturating_add_signed(*delta);
                audio.music_volume = step_to_volume(step);
            }
            SettingsAction::SfxStep(delta) => {
                let step = volume_to_step(audio.sfx_volume).saturating_add_signed(*delta);
                audio.sfx_volume = step_to_volume(step);
            }
            SettingsAction::ToggleMute => audio.muted = !audio.muted,
            SettingsAction::ToggleReducedMotion => {
                accessibility.reduced_motion = !accessibility.reduced_motion;
            }
            SettingsAction::ToggleHighContrast => {
                accessibility.high_contrast = !accessibility.high_contrast;
            }
            SettingsAction::Back => commands.remove_resource::<SettingsOpen>(),
        }
    }
}

/// Keeps the step values, the mute label, and the two accessibility toggle
/// labels in sync with [`AudioSettings`] (M-key mutes included) and
/// [`AccessibilityPreferences`].
fn update_labels(
    audio: Res<AudioSettings>,
    accessibility: Res<AccessibilityPreferences>,
    mut labels: Query<(&mut Text, &SettingsLabel)>,
) {
    if !audio.is_changed() && !accessibility.is_changed() {
        return;
    }
    for (mut text, label) in &mut labels {
        text.0 = match label {
            SettingsLabel::Music => volume_to_step(audio.music_volume).to_string(),
            SettingsLabel::Sfx => volume_to_step(audio.sfx_volume).to_string(),
            SettingsLabel::Mute => mute_label(audio.muted).to_string(),
            SettingsLabel::ReducedMotion => {
                reduced_motion_label(accessibility.reduced_motion).to_string()
            }
            SettingsLabel::HighContrast => {
                high_contrast_label(accessibility.high_contrast).to_string()
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioSettings;
    use crate::core::CorePlugin;
    use crate::save::MemoryBackend;
    use bevy::state::app::StatesPlugin;
    use std::sync::{Arc, Mutex};

    // ── pure stepper math ───────────────────────────────────────────────

    #[test]
    fn steps_map_linearly_onto_volumes() {
        assert_eq!(step_to_volume(0), 0.0);
        assert_eq!(step_to_volume(5), 0.5);
        assert_eq!(step_to_volume(10), 1.0);
        assert_eq!(step_to_volume(99), 1.0, "out-of-range steps clamp");
    }

    #[test]
    fn volumes_map_back_to_their_nearest_step() {
        assert_eq!(volume_to_step(0.0), 0);
        assert_eq!(volume_to_step(0.5), 5);
        assert_eq!(volume_to_step(1.0), 10);
        assert_eq!(volume_to_step(0.54), 5, "rounds to the nearest step");
        assert_eq!(volume_to_step(-3.0), 0, "clamps below");
        assert_eq!(volume_to_step(7.0), 10, "clamps above");
    }

    #[test]
    fn every_step_roundtrips_through_the_volume_mapping() {
        for step in 0..=VOLUME_STEPS {
            assert_eq!(volume_to_step(step_to_volume(step)), step);
        }
    }

    // ── JSON roundtrip through the storage backend ──────────────────────

    fn in_memory_store() -> (SettingsStore, Arc<Mutex<Option<String>>>) {
        let cell = Arc::new(Mutex::new(None));
        (
            SettingsStore::with_backend(MemoryBackend(Arc::clone(&cell))),
            cell,
        )
    }

    #[test]
    fn settings_roundtrip_through_the_storage_backend() {
        let (store, _cell) = in_memory_store();
        let audio = AudioSettings {
            music_volume: 0.3,
            sfx_volume: 0.9,
            muted: true,
        };
        let accessibility = AccessibilityPreferences {
            reduced_motion: true,
            high_contrast: false,
        };
        let save = SettingsSave::capture(&audio, &accessibility);
        store.store(&save.to_json().expect("plain data serializes"));
        let restored = store
            .load()
            .as_deref()
            .and_then(SettingsSave::from_json)
            .expect("own JSON loads");
        assert_eq!(restored, save);
        assert_eq!(restored.audio_settings(), audio);
        assert_eq!(restored.accessibility_preferences(), accessibility);
    }

    #[test]
    fn accessibility_preferences_default_to_false() {
        assert_eq!(
            AccessibilityPreferences::default(),
            AccessibilityPreferences {
                reduced_motion: false,
                high_contrast: false,
            }
        );
    }

    /// A #30-era, audio-only blob (no `reduced_motion`/`high_contrast` at
    /// all) migrates: every audio value carries over exactly, and both new
    /// accessibility fields default to `false` — never a panic, never a
    /// dropped audio value.
    #[test]
    fn v1_audio_only_settings_migrate_with_defaulted_accessibility() {
        let fixture = r#"{"version":1,"music":7,"sfx":2,"muted":true}"#;
        let migrated = SettingsSave::from_json(fixture).expect("a v1 blob migrates");
        assert_eq!(
            migrated,
            SettingsSave {
                version: SETTINGS_VERSION,
                music: 7,
                sfx: 2,
                muted: true,
                reduced_motion: false,
                high_contrast: false,
            }
        );
        assert_eq!(
            migrated.audio_settings(),
            AudioSettings {
                music_volume: 0.7,
                sfx_volume: 0.2,
                muted: true,
            },
            "audio values survive the migration byte-for-byte"
        );
        assert_eq!(
            migrated.accessibility_preferences(),
            AccessibilityPreferences::default(),
            "accessibility fields default safely on a pre-#191 blob"
        );
    }

    #[test]
    fn corrupt_or_future_versioned_settings_fall_back_to_none() {
        for bad in [
            "",
            "not json",
            "{",
            "null",
            "[]",
            r#"{"music":5,"sfx":5,"muted":false}"#,
            r#"{"version":"nope","music":5,"sfx":5,"muted":false}"#,
            r#"{"version":0,"music":5,"sfx":5,"muted":false}"#,
            r#"{"version":3,"music":5,"sfx":5,"muted":false,"reduced_motion":false,"high_contrast":false}"#,
        ] {
            assert!(
                SettingsSave::from_json(bad).is_none(),
                "expected None for {bad:?}"
            );
        }
    }

    // ── app-level: overlay, live changes, persistence ───────────────────

    fn test_app() -> (App, Arc<Mutex<Option<String>>>) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app.init_resource::<AudioSettings>();
        let (store, cell) = in_memory_store();
        app.insert_resource(store);
        app.add_plugins(SettingsPlugin);
        (app, cell)
    }

    fn overlay_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<SettingsOverlay>>()
            .iter(app.world())
            .count()
    }

    fn find_button(app: &mut App, action: SettingsAction) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &SettingsAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(entity, _)| entity)
            .expect("settings button exists")
    }

    fn click(app: &mut App, button: Entity) {
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.world_mut().entity_mut(button).insert(Interaction::None);
        app.update();
    }

    fn audio(app: &App) -> AudioSettings {
        *app.world().resource::<AudioSettings>()
    }

    fn accessibility(app: &App) -> AccessibilityPreferences {
        *app.world().resource::<AccessibilityPreferences>()
    }

    #[test]
    fn stored_settings_load_at_startup() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin));
        app.init_resource::<AudioSettings>();
        let (store, _cell) = in_memory_store();
        store.store(
            &SettingsSave {
                version: SETTINGS_VERSION,
                music: 2,
                sfx: 10,
                muted: true,
                reduced_motion: true,
                high_contrast: true,
            }
            .to_json()
            .expect("plain data serializes"),
        );
        app.insert_resource(store);
        app.add_plugins(SettingsPlugin);
        app.update();
        assert_eq!(
            audio(&app),
            AudioSettings {
                music_volume: 0.2,
                sfx_volume: 1.0,
                muted: true,
            }
        );
        assert_eq!(
            accessibility(&app),
            AccessibilityPreferences {
                reduced_motion: true,
                high_contrast: true,
            },
            "both accessibility preferences load from the stored blob \
             (this is the app-level analogue of a browser reload: a fresh \
             app reading the same stored blob at Startup)"
        );
    }

    #[test]
    fn a_corrupt_settings_blob_keeps_the_defaults() {
        let (mut app, cell) = test_app();
        *cell.lock().expect("test store lock") = Some("garbage".to_string());
        app.update();
        assert_eq!(audio(&app), AudioSettings::default());
        assert_eq!(accessibility(&app), AccessibilityPreferences::default());
    }

    #[test]
    fn opening_and_closing_the_overlay_spawns_and_despawns_it() {
        let (mut app, _cell) = test_app();
        app.update();
        assert_eq!(overlay_count(&mut app), 0);
        app.insert_resource(SettingsOpen);
        app.update();
        assert_eq!(overlay_count(&mut app), 1);
        let back = find_button(&mut app, SettingsAction::Back);
        // One-shot press: the button despawns with the overlay, so the
        // interaction is never reset.
        app.world_mut()
            .entity_mut(back)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
        assert_eq!(overlay_count(&mut app), 0, "Înapoi closes the overlay");
        assert!(app.world().get_resource::<SettingsOpen>().is_none());
    }

    /// #216: opening the overlay must focus its first control immediately
    /// -- see `autofocus_settings_overlay`'s doc comment for why a modal
    /// group needs this rather than relying on `TabNavigation`'s own modal
    /// handling.
    #[test]
    fn opening_the_overlay_autofocuses_its_first_control() {
        let (mut app, _cell) = test_app();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();

        let music_decrease = find_button(&mut app, SettingsAction::MusicStep(-1));
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(music_decrease),
            "the overlay's first control (Music -) must be focused the instant it opens"
        );
    }

    /// #216: a screen's own non-modal `TabGroup` (simulated here, since this
    /// minimal `test_app` never spawns the main menu) must stay unreachable
    /// while the modal settings overlay is open — the whole point of
    /// `TabGroup::modal()`.
    #[test]
    fn tab_inside_the_open_overlay_never_reaches_a_screen_behind_it() {
        let (mut app, _cell) = test_app();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.update();

        // A stand-in for "whatever screen is behind the overlay" (main menu
        // or the paused fight), its own non-modal focus region.
        let screen_button = app
            .world_mut()
            .spawn((
                Button,
                crate::ui_widgets::focus::Focusable,
                crate::ui_widgets::focus::TabIndex(0),
                crate::ui_widgets::focus::TabGroup::new(0),
            ))
            .id();

        app.insert_resource(SettingsOpen);
        app.update();

        for _ in 0..12 {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Tab);
            app.update();
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
            keys.clear();
            assert_ne!(
                app.world().resource::<InputFocus>().get(),
                Some(screen_button),
                "Tab must never reach the screen behind an open modal overlay"
            );
        }
    }

    /// #216: audio settings must be operable keyboard-only -- Enter on the
    /// focused Music `+` stepper must step the volume exactly like a click.
    #[test]
    fn enter_on_a_focused_stepper_steps_the_volume_like_a_click() {
        let (mut app, _cell) = test_app();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();

        let before = app.world().resource::<AudioSettings>().music_volume;
        let music_increase = find_button(&mut app, SettingsAction::MusicStep(1));
        app.world_mut()
            .insert_resource(InputFocus::from_entity(music_increase));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();

        assert!(
            app.world().resource::<AudioSettings>().music_volume > before,
            "Enter on the focused Music + must raise the volume like a click"
        );
    }

    /// #216: closing the overlay must clear focus (the overlay's whole
    /// subtree — including whatever was focused — despawns with it), so the
    /// next Tab on the screen behind it starts fresh at that screen's own
    /// first control.
    #[test]
    fn closing_the_overlay_clears_focus() {
        let (mut app, _cell) = test_app();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();
        assert!(app.world().resource::<InputFocus>().get().is_some());

        let back = find_button(&mut app, SettingsAction::Back);
        app.world_mut()
            .entity_mut(back)
            .insert(Interaction::Pressed);
        app.update();
        app.update();

        assert_eq!(app.world().resource::<InputFocus>().get(), None);
    }

    /// #216: this overlay opens from both the main menu and the paused
    /// fight's own **Setări** button; in the second case the pause
    /// overlay's `TabGroup::modal()` is still alive underneath when this
    /// one closes. `despawn_overlay` must detect that (generically -- it
    /// never names `combat::pause`) and re-focus into it rather than
    /// leaking focus to the fight screen's own HUD/palette.
    #[test]
    fn closing_the_overlay_refocuses_another_still_open_modal_if_present() {
        let (mut app, _cell) = test_app();
        app.update();

        // A stand-in for the pause overlay's own modal panel: a modal
        // `TabGroup` that stays alive underneath this one.
        let other_modal_root = app
            .world_mut()
            .spawn(crate::ui_widgets::focus::TabGroup::modal())
            .id();
        let other_modal_child = app
            .world_mut()
            .spawn((
                Button,
                crate::ui_widgets::focus::Focusable,
                crate::ui_widgets::focus::TabIndex(0),
                ChildOf(other_modal_root),
            ))
            .id();

        app.insert_resource(SettingsOpen);
        app.update();
        let back = find_button(&mut app, SettingsAction::Back);
        app.world_mut()
            .entity_mut(back)
            .insert(Interaction::Pressed);
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(other_modal_child),
            "closing settings-over-pause must refocus the still-open pause modal, \
             never clear to the fight screen behind it"
        );
    }

    #[test]
    fn volume_steppers_apply_live_and_clamp_at_the_ends() {
        let (mut app, _cell) = test_app();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();

        let up = find_button(&mut app, SettingsAction::MusicStep(1));
        click(&mut app, up); // 5 -> 6
        assert_eq!(audio(&app).music_volume, 0.6);
        for _ in 0..8 {
            click(&mut app, up);
        }
        assert_eq!(audio(&app).music_volume, 1.0, "clamps at step 10");

        let down = find_button(&mut app, SettingsAction::SfxStep(-1));
        for _ in 0..12 {
            click(&mut app, down);
        }
        assert_eq!(audio(&app).sfx_volume, 0.0, "clamps at step 0");
    }

    #[test]
    fn the_mute_toggle_flips_the_master_mute() {
        let (mut app, _cell) = test_app();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();
        let toggle = find_button(&mut app, SettingsAction::ToggleMute);
        click(&mut app, toggle);
        assert!(audio(&app).muted);
        click(&mut app, toggle);
        assert!(!audio(&app).muted);
    }

    #[test]
    fn the_reduced_motion_toggle_flips_independently_of_high_contrast() {
        let (mut app, _cell) = test_app();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();
        let toggle = find_button(&mut app, SettingsAction::ToggleReducedMotion);
        click(&mut app, toggle);
        assert_eq!(
            accessibility(&app),
            AccessibilityPreferences {
                reduced_motion: true,
                high_contrast: false,
            }
        );
        click(&mut app, toggle);
        assert_eq!(accessibility(&app), AccessibilityPreferences::default());
    }

    #[test]
    fn the_high_contrast_toggle_flips_independently_of_reduced_motion() {
        let (mut app, _cell) = test_app();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();
        let toggle = find_button(&mut app, SettingsAction::ToggleHighContrast);
        click(&mut app, toggle);
        assert_eq!(
            accessibility(&app),
            AccessibilityPreferences {
                reduced_motion: false,
                high_contrast: true,
            }
        );
        click(&mut app, toggle);
        assert_eq!(accessibility(&app), AccessibilityPreferences::default());
    }

    #[test]
    fn every_change_persists_to_the_settings_store() {
        let (mut app, cell) = test_app();
        app.update();
        assert_eq!(
            *cell.lock().expect("test store lock"),
            None,
            "startup alone writes nothing"
        );
        app.insert_resource(SettingsOpen);
        app.update();
        let up = find_button(&mut app, SettingsAction::MusicStep(1));
        click(&mut app, up);
        let stored = cell
            .lock()
            .expect("test store lock")
            .as_deref()
            .and_then(SettingsSave::from_json)
            .expect("the change is persisted");
        assert_eq!(stored.music, 6);
    }

    #[test]
    fn accessibility_toggles_persist_to_the_settings_store() {
        let (mut app, cell) = test_app();
        app.update();
        app.insert_resource(SettingsOpen);
        app.update();

        let reduced_motion = find_button(&mut app, SettingsAction::ToggleReducedMotion);
        click(&mut app, reduced_motion);
        let high_contrast = find_button(&mut app, SettingsAction::ToggleHighContrast);
        click(&mut app, high_contrast);

        let stored = cell
            .lock()
            .expect("test store lock")
            .as_deref()
            .and_then(SettingsSave::from_json)
            .expect("the accessibility changes are persisted");
        assert!(stored.reduced_motion);
        assert!(stored.high_contrast);
    }

    #[test]
    fn game_over_deletes_the_run_save_but_not_the_settings() {
        use crate::core::GameState;
        use crate::save::{SavePlugin, SaveStore};

        let (mut app, settings_cell) = test_app();
        app.add_plugins(SavePlugin);
        let (run_store, run_cell) = SaveStore::in_memory();
        run_store.store(r#"{"pretend":"run save"}"#);
        app.insert_resource(run_store);
        *settings_cell.lock().expect("test store lock") = Some(
            SettingsSave {
                version: SETTINGS_VERSION,
                music: 3,
                sfx: 7,
                muted: false,
                reduced_motion: true,
                high_contrast: true,
            }
            .to_json()
            .expect("plain data serializes"),
        );
        app.update();

        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::GameOver);
        app.update();

        assert_eq!(
            *run_cell.lock().expect("test store lock"),
            None,
            "game over deletes the run save"
        );
        let settings = settings_cell
            .lock()
            .expect("test store lock")
            .as_deref()
            .and_then(SettingsSave::from_json)
            .expect("the settings (audio + accessibility) survive game over");
        assert_eq!((settings.music, settings.sfx), (3, 7));
        assert!(
            settings.reduced_motion && settings.high_contrast,
            "accessibility preferences are untouched by run deletion, same as audio"
        );
    }
}
