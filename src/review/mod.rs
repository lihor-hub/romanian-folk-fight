//! Review-only deterministic seam (#187), compiled in only behind the
//! `review` cargo feature -- see `Cargo.toml`'s `[features]` doc comment.
//! **Absent entirely** from an ordinary `cargo build`/`cargo build --release`
//! or a plain `trunk build --release`: this whole module is behind
//! `#[cfg(feature = "review")]` (this file's `#![cfg(...)]`, plus the
//! matching `#[cfg(feature = "review")] pub mod review;` in `src/lib.rs`), so
//! the seam is not merely hidden behind a runtime flag -- the code does not
//! exist in the compiled artifact at all unless a build opts in with
//! `--features review`. The only build that does is the dedicated review
//! WASM build `cargo xtask web-smoke --scenario gold-journey` produces (see
//! `xtask/src/web_smoke/gold_journey.rs`), served from its own `dist-gold-journey/`
//! directory, never `dist/`.
//!
//! # Contract: what the harness can do through this seam
//!
//! The browser harness (a real, freshly launched Chrome, same as #168's
//! `cold-menu`) talks to this seam entirely through `window.localStorage`,
//! reusing the same web platform API `src/save`'s wasm backend already uses
//! for persistence -- no new dependency, no `wasm-bindgen` exports to wire
//! through Trunk's bundling.
//!
//! - **Commands** (harness -> game): the harness JS calls
//!   `localStorage.setItem(REVIEW_COMMAND_KEY, <json>)` where `<json>` is one
//!   JSON-serialized [`ReviewCommand`]. [`poll_review_commands`] drains at
//!   most one pending command per frame (reads then immediately removes the
//!   key, so a command is never re-applied) and dispatches it:
//!   - `seedCombat`: inserts a fixed-seed [`crate::combat::CombatRng`] --
//!     the *same* seam `combat::systems::setup_combat` already documents
//!     ("tests insert a fixed seed for deterministic duels"): `setup_combat`
//!     only seeds `CombatRng` from the clock when the resource is *absent*,
//!     so calling this before `ConfirmHero` makes the whole duel
//!     deterministic without touching any production system.
//!   - `selectPreset`: calls the exact same [`crate::creation::CharacterDraft::select_choice`]
//!     the preset buttons call (see `creation::handle_creation_actions`'s
//!     `CreationAction::SelectChoice` arm) -- not a screen transition, so it
//!     is plain in-screen state seeding, not a `NextState` write.
//!   - `pressButton`: sets `Interaction::Pressed` on the named screen
//!     button's actual entity -- deterministic *input* seeding, the same
//!     `Interaction` write a real click produces (and the exact mechanism
//!     every screen's own unit tests already use to press buttons). The
//!     production click handler then does everything a player's click does:
//!     its domain side effects (run reset on **Luptă nouă**, the
//!     `PlayerCharacter`/loadout insert + autosave on **Începe lupta**, ...)
//!     *and* emitting the [`crate::flow::FlowIntent`] the flow table routes.
//!     This is deliberately not a "write a FlowIntent directly" channel:
//!     the flow module's ordering contract requires the emitter to apply
//!     its domain side effect *before* the intent, and only the real
//!     handler knows what that is -- a raw intent write would, e.g., enter
//!     the fight without a `PlayerCharacter` and the arena would never
//!     spawn. [`parse_button`] recognizes only the player-facing
//!     navigation buttons; a disabled button is refused (with a `warn!`),
//!     exactly like a real click on it would be. This module never writes
//!     `NextState<GameState>` -- navigation is always the existing
//!     [`crate::flow::apply_flow_intents`] (#166) applying an intent the
//!     production button handler emitted.
//!   - `setTimePaused`: pauses/unpauses Bevy's `Time<Virtual>` so the
//!     harness can capture a byte-stable screenshot on screens with
//!     continuous idle animation; see [`ReviewCommand::SetTimePaused`].
//!   - `setAutoplay`: toggles [`ReviewAutoplay`], which
//!     [`autoplay_player_turn`] reads to script the player's side of the
//!     duel with a fixed, deterministic policy (Rest below the quick-strike
//!     stamina cost, QuickStrike otherwise) by writing the *same*
//!     `combat::systems::PlayerActionEvent` the HUD's action buttons write --
//!     deterministic input seeding, not a bypass of the combat resolver.
//! - **Readiness** (game -> harness): [`publish_current_screen`] writes the
//!   current [`crate::core::GameState`] (its `Debug` name, e.g.
//!   `"CharacterCreation"`) to `localStorage` under [`REVIEW_SCREEN_KEY`]
//!   every frame, so the harness can poll a real, semantic "which screen is
//!   this" signal instead of coordinate-only timing, on top of #168's
//!   existing frame-stability contract (`web_smoke::browser::Checkpoint::wait_for_frame`
//!   + screenshot-stability streak).
//!
//! # Why `localStorage`, not a `wasm-bindgen` export
//!
//! Trunk generates its own JS loader glue for the wasm module; making a
//! `#[wasm_bindgen]`-exported Rust function reachable as a plain
//! `window.__foo()` call from outside that generated module is extra
//! bundling wiring this issue doesn't need. `window.localStorage` is already
//! a stable, typed `web-sys` API this crate depends on for `src/save`'s web
//! backend (`Storage`/`Window` features, already in `Cargo.toml`), so no new
//! dependency is added for the `review` feature at all.
#![cfg(feature = "review")]

use bevy::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::arena::fx::ParallaxLayer;
use crate::arena::{ENEMY_ANCHOR, PLAYER_ANCHOR};
use crate::character::{EnemyFighter, PlayerFighter, Stamina};
use crate::combat::action_palette::ActionButton;
use crate::combat::engine::QUICK_STRIKE_COST;
use crate::combat::systems::{CombatPresentation, PlayerActionEvent};
use crate::combat::{CombatAction, CombatRng, CombatSide, CombatTurn};
use crate::core::{GameState, LetterboxRect, WorldCamera};
use crate::creation::{CharacterDraft, CreationAction, HeroChoice, HeroPreset};
use crate::menu::{DisabledButton, MenuAction};
use crate::progression::result_ui::{GameOverAction, ResultAction};
use crate::progression::victory_ui::VictoryAction;
use crate::settings::AccessibilityPreferences;
use crate::shop::ShopAction;
use crate::theme::Palette;

/// `localStorage` key the harness writes a pending [`ReviewCommand`] to.
/// Duplicated as a plain string in `xtask/src/web_smoke/gold_journey.rs`
/// (which cannot depend on this crate's `review` feature -- browser tooling
/// stays in the dev-only `xtask` crate, see its `Cargo.toml`'s dependency
/// note), the same cross-referenced-string-literal pattern
/// `cold_menu::REQUIRED_ASSETS` already uses for `core::UI_FONT_PATH` etc.
pub const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
/// `localStorage` key this seam publishes the current [`GameState`]'s
/// `Debug` name to, every frame. See [`REVIEW_COMMAND_KEY`]'s doc comment
/// for why this is duplicated as a plain string on the `xtask` side.
pub const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
/// `localStorage` key this seam publishes a [`MotionSnapshot`] to every
/// frame the arena is up (cleared otherwise) -- added for #200's
/// `reduced-motion-fight` browser scenario, which needs exact fighter/
/// camera/parallax positions to assert the reduced-motion treatment
/// precisely instead of diffing screenshots. See [`REVIEW_COMMAND_KEY`]'s
/// doc comment for why this is duplicated as a plain string on the `xtask`
/// side.
pub const REVIEW_MOTION_KEY: &str = "rff_review_motion_v1";
/// `localStorage` key this seam publishes a [`PaletteSnapshot`] to every
/// frame the fight HUD's action bar is up (cleared otherwise) -- added for
/// #189's `fight-palette-desktop` browser scenario. The overflow/clipping
/// check the scenario needs ("does every action button render inside the
/// letterboxed stage rect") is computed once here, in native Bevy space with
/// real `ComputedNode`/`UiGlobalTransform` values, rather than duplicated as
/// pixel-math on the `xtask` side (which would have to guess this crate's UI
/// coordinate conventions) -- the same reasoning [`REVIEW_MOTION_KEY`]
/// documents for reduced-motion displacement. See [`REVIEW_COMMAND_KEY`]'s
/// doc comment for why the key itself is duplicated as a plain string on the
/// `xtask` side.
pub const REVIEW_PALETTE_KEY: &str = "rff_review_palette_v1";
/// `localStorage` key this seam publishes a [`ThemeSnapshot`] to every
/// frame -- added for #214's `high-contrast` browser scenario, which needs
/// to confirm the active [`Palette`] resource actually switched to the
/// high-contrast variant, rather than guessing from a screenshot pixel
/// (font antialiasing and JPEG-free-but-still-lossy PNG capture make an
/// exact color read off a screenshot unreliable) -- the same
/// telemetry-over-pixel-diffing reasoning [`REVIEW_MOTION_KEY`]/
/// [`REVIEW_PALETTE_KEY`] already document. Unlike those two, this snapshot
/// is published unconditionally (the theme applies on every screen, not
/// just the arena/fight HUD), so there is no corresponding `clear_theme`.
/// See [`REVIEW_COMMAND_KEY`]'s doc comment for why the key itself is
/// duplicated as a plain string on the `xtask` side.
pub const REVIEW_THEME_KEY: &str = "rff_review_theme_v1";

/// One command the harness can queue through [`REVIEW_COMMAND_KEY`]. Plain
/// JSON via `serde`, tagged by `cmd` so the wire format is a flat, readable
/// object, e.g. `{"cmd":"seedCombat","seed":1234}`.
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum ReviewCommand {
    SeedCombat {
        seed: u64,
    },
    SelectPreset {
        preset: String,
    },
    PressButton {
        button: String,
    },
    SetAutoplay {
        enabled: bool,
    },
    /// Pauses/unpauses Bevy's `Time<Virtual>` -- the clock every
    /// presentation animation (parallax drift, idle sprite frames,
    /// presentation/fight-end timers) ticks from. The harness wraps each
    /// screenshot capture in a pause/unpause pair so #168's byte-identical-
    /// frames stability streak can land on screens with continuous idle
    /// animation (the fight screen's parallax layers sway every frame
    /// otherwise -- see `arena::fx::drift_parallax_layers`). This is a
    /// standard Bevy API (`Time<Virtual>::pause`), not a game-logic change:
    /// game systems all read the same paused clock, so state simply holds
    /// still while the capture happens.
    SetTimePaused {
        paused: bool,
    },
}

/// Whether [`autoplay_player_turn`] is currently scripting the player's
/// combat turns. Off by default -- a harness must opt in with
/// `{"cmd":"setAutoplay","enabled":true}` after seeding combat and entering
/// the fight, so it can still capture the fresh, untouched fight-start
/// checkpoint first.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReviewAutoplay(pub bool);

pub struct ReviewPlugin;

impl Plugin for ReviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReviewAutoplay>()
            // Idempotent re-registrations of what the systems below read and
            // write (the real app's CreationPlugin/CombatPlugin already
            // provide both), so this plugin never depends on plugin order --
            // the same self-containment pattern CreationPlugin itself uses
            // for `SaveRequested`.
            .init_resource::<CharacterDraft>()
            // #214: publish_theme_state reads both; idempotent with
            // ThemePlugin/SettingsPlugin's own registrations.
            .init_resource::<Palette>()
            .init_resource::<AccessibilityPreferences>()
            .add_message::<PlayerActionEvent>()
            .add_systems(
                Update,
                (
                    // Before the emission set so a same-frame `pressButton`'s
                    // `Interaction::Pressed` write is observed by the screen's
                    // own click handler (all of which live in
                    // `FlowIntentEmission`) *this* frame -- `bevy_ui`'s focus
                    // system resets a pressed interaction the pointer isn't
                    // actually holding on the next frame's `PreUpdate`, so the
                    // write must land within the same `Update` pass.
                    poll_review_commands.before(crate::flow::FlowIntentEmission),
                    publish_current_screen,
                    publish_motion_state,
                    publish_palette_state,
                    publish_theme_state,
                    autoplay_player_turn,
                ),
            );
    }
}

/// One parallax layer's rest and current x, published as part of a
/// [`MotionSnapshot`].
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq)]
struct ParallaxSample {
    base_x: f32,
    x: f32,
}

/// Everything the `reduced-motion-fight` scenario (#200) needs to assert the
/// reduced-motion treatment precisely: both fighters' root transform (and
/// their rest anchors, so the scenario can compute an exact offset without
/// duplicating `arena`'s anchor constants), the camera translation (screen
/// shake's target), and every parallax layer's rest/current x. Published
/// under [`REVIEW_MOTION_KEY`] every frame the arena is up.
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct MotionSnapshot {
    player_x: f32,
    player_anchor_x: f32,
    enemy_x: f32,
    enemy_anchor_x: f32,
    camera_x: f32,
    camera_y: f32,
    parallax: Vec<ParallaxSample>,
}

/// Publishes a [`MotionSnapshot`] every frame the arena's fighters/camera
/// exist (outside the fight, e.g. on the menu, clears the key instead so a
/// scenario can't mistake a stale snapshot from a previous fight for the
/// current one).
fn publish_motion_state(
    players: Query<&Transform, (With<PlayerFighter>, Without<EnemyFighter>)>,
    enemies: Query<&Transform, (With<EnemyFighter>, Without<PlayerFighter>)>,
    cameras: Query<&Transform, With<WorldCamera>>,
    parallax: Query<(&ParallaxLayer, &Transform)>,
) {
    let (Ok(player), Ok(enemy)) = (players.single(), enemies.single()) else {
        clear_motion();
        return;
    };
    let (camera_x, camera_y) = cameras
        .single()
        .map(|transform| (transform.translation.x, transform.translation.y))
        .unwrap_or((0.0, 0.0));
    let snapshot = MotionSnapshot {
        player_x: player.translation.x,
        player_anchor_x: PLAYER_ANCHOR.translation.x,
        enemy_x: enemy.translation.x,
        enemy_anchor_x: ENEMY_ANCHOR.translation.x,
        camera_x,
        camera_y,
        parallax: parallax
            .iter()
            .map(|(layer, transform)| ParallaxSample {
                base_x: layer.base_x,
                x: transform.translation.x,
            })
            .collect(),
    };
    match serde_json::to_string(&snapshot) {
        Ok(json) => publish_motion(&json),
        Err(_) => clear_motion(),
    }
}

/// Everything the `fight-palette-desktop` scenario (#189) needs to assert
/// the desktop action bar renders without overflow/clipping: how many
/// [`ActionButton`] entities exist right now, and whether every one of them
/// rendered entirely inside the letterboxed stage rect
/// ([`crate::core::LetterboxRect`]). `fits` is computed here (in native Bevy
/// UI space, from real `ComputedNode`/`UiGlobalTransform` values) rather
/// than left for the browser harness to re-derive from duplicated layout
/// constants -- see [`REVIEW_PALETTE_KEY`]'s doc comment. Published under
/// [`REVIEW_PALETTE_KEY`] every frame the fight HUD's action bar is up.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq)]
struct PaletteSnapshot {
    /// How many action buttons currently exist (spawned = rendered; #189's
    /// palette never despawns/hides a button to make it fit).
    button_count: usize,
    /// Whether every button's on-screen box lies entirely within the
    /// letterboxed stage rect -- `false` (or `button_count == 0`) means the
    /// scenario must fail: an overflowing or clipped action bar.
    fits: bool,
}

/// The `Rect` a UI node actually occupies on screen, in the same logical-
/// pixel space [`LetterboxRect`] is expressed in: `ComputedNode::size` is in
/// physical pixels and `UiGlobalTransform`'s translation places the node's
/// center in physical-pixel space (matching `ComputedNode::contains_point`'s
/// own convention), so both are scaled back to logical pixels by the node's
/// own `inverse_scale_factor` before building the rect.
fn logical_node_rect(transform: &UiGlobalTransform, node: &ComputedNode) -> Rect {
    let scale = node.inverse_scale_factor();
    Rect::from_center_size(transform.translation * scale, node.size() * scale)
}

/// Publishes a [`PaletteSnapshot`] every frame at least one [`ActionButton`]
/// exists (clears the key otherwise, e.g. outside the fight screen, so a
/// scenario can't mistake a stale snapshot from a previous fight for the
/// current one).
fn publish_palette_state(
    letterbox: Option<Res<LetterboxRect>>,
    buttons: Query<(&UiGlobalTransform, &ComputedNode), With<ActionButton>>,
) {
    let Some(letterbox) = letterbox else {
        clear_palette();
        return;
    };
    let stage = Rect::from_corners(letterbox.position, letterbox.position + letterbox.size);
    let mut button_count = 0usize;
    let mut extent: Option<Rect> = None;
    for (transform, node) in &buttons {
        button_count += 1;
        let rect = logical_node_rect(transform, node);
        extent = Some(match extent {
            Some(union) => union.union(rect),
            None => rect,
        });
    }
    if button_count == 0 {
        clear_palette();
        return;
    }
    let fits =
        extent.is_some_and(|extent| stage.contains(extent.min) && stage.contains(extent.max));
    let snapshot = PaletteSnapshot { button_count, fits };
    match serde_json::to_string(&snapshot) {
        Ok(json) => publish_palette(&json),
        Err(_) => clear_palette(),
    }
}

/// Everything the `high-contrast` scenario (#214) needs to confirm the
/// active [`Palette`] actually switched, published as exact `0..=255` sRGB
/// triples read straight from the live resource rather than sampled off a
/// screenshot. Published under [`REVIEW_THEME_KEY`] every frame.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq)]
struct ThemeSnapshot {
    /// Mirrors `AccessibilityPreferences::high_contrast` -- the input the
    /// scenario seeded, echoed back so a mismatch (preference on, palette
    /// still normal) is visible directly in the snapshot.
    high_contrast: bool,
    hp_fill: [u8; 3],
    bar_track: [u8; 3],
    text_primary: [u8; 3],
}

/// One color's `0..=255` sRGB triple (alpha dropped -- every token this
/// snapshot carries is opaque).
fn srgb_u8(color: Color) -> [u8; 3] {
    let srgba = color.to_srgba();
    [
        (srgba.red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (srgba.green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (srgba.blue.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn publish_theme_state(palette: Res<Palette>, accessibility: Res<AccessibilityPreferences>) {
    let snapshot = ThemeSnapshot {
        high_contrast: accessibility.high_contrast,
        hp_fill: srgb_u8(palette.hp_fill),
        bar_track: srgb_u8(palette.bar_track),
        text_primary: srgb_u8(palette.text_primary),
    };
    if let Ok(json) = serde_json::to_string(&snapshot) {
        publish_theme(&json);
    }
}

/// Maps a preset's exact display name (see [`HeroPreset::name`]) to the
/// variant -- the same string a `selectPreset` review command carries.
fn parse_preset(name: &str) -> Option<HeroPreset> {
    HeroPreset::ALL
        .into_iter()
        .find(|preset| preset.name() == name)
}

/// One pressable screen button, resolved from a `pressButton` command's
/// name. Covers exactly the player-facing navigation buttons of the five
/// journey screens (plus game-over/victory for later scenarios); in-screen
/// editing buttons (attribute steppers, shop purchases, ...) are deliberately
/// not exposed -- in-screen state is seeded through dedicated commands like
/// `selectPreset` instead.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ReviewButton {
    Menu(MenuAction),
    Creation(CreationAction),
    Result(ResultAction),
    GameOver(GameOverAction),
    Victory(VictoryAction),
    Shop(ShopAction),
}

/// Maps a `pressButton` command's `button` field to the screen button it
/// presses. An unrecognized name returns `None`, which
/// [`poll_review_commands`] logs and drops.
fn parse_button(name: &str) -> Option<ReviewButton> {
    match name {
        "NewGame" => Some(ReviewButton::Menu(MenuAction::NewGame)),
        "Continue" => Some(ReviewButton::Menu(MenuAction::Continue)),
        "ConfirmHero" => Some(ReviewButton::Creation(CreationAction::Confirm)),
        "CreationBack" => Some(ReviewButton::Creation(CreationAction::Back)),
        "GoToShop" => Some(ReviewButton::Result(ResultAction::GoToShop)),
        "NextFight" => Some(ReviewButton::Result(ResultAction::NextFight)),
        "GameOverBackToMenu" => Some(ReviewButton::GameOver(GameOverAction::BackToMenu)),
        "VictoryNextLap" => Some(ReviewButton::Victory(VictoryAction::NextLap)),
        "VictoryBackToMenu" => Some(ReviewButton::Victory(VictoryAction::BackToMenu)),
        "BackToArena" => Some(ReviewButton::Shop(ShopAction::BackToArena)),
        _ => None,
    }
}

/// Everything [`poll_review_commands`] needs to find and press one screen
/// button exactly like a click: the button's `Interaction` (mutated in
/// place, no command flush needed), its disabled marker, and whichever
/// action component its screen tagged it with.
type PressableButton = (
    &'static mut Interaction,
    Has<DisabledButton>,
    Option<&'static MenuAction>,
    Option<&'static CreationAction>,
    Option<&'static ResultAction>,
    Option<&'static GameOverAction>,
    Option<&'static VictoryAction>,
    Option<&'static ShopAction>,
);

/// Drains and applies at most one pending [`ReviewCommand`] this frame (see
/// the module docs for what each variant does). A malformed or rejected
/// command is logged via `warn!` and otherwise ignored -- never a panic, so
/// a harness bug fails loudly in `console.log`/the checkpoint's retained
/// `console.log` artifact rather than crashing the review build.
fn poll_review_commands(
    mut commands: Commands,
    mut draft: ResMut<CharacterDraft>,
    mut autoplay: ResMut<ReviewAutoplay>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut buttons: Query<PressableButton, With<Button>>,
) {
    let Some(raw) = take_pending_command() else {
        return;
    };
    match serde_json::from_str::<ReviewCommand>(&raw) {
        Ok(ReviewCommand::SeedCombat { seed }) => {
            commands.insert_resource(CombatRng(ChaCha8Rng::seed_from_u64(seed)));
        }
        Ok(ReviewCommand::SelectPreset { preset }) => match parse_preset(&preset) {
            Some(preset) => draft.select_choice(HeroChoice::Preset(preset)),
            None => warn!("review: selectPreset(\"{preset}\") is not a known hero preset"),
        },
        Ok(ReviewCommand::PressButton { button }) => match parse_button(&button) {
            Some(target) => press_button(&button, target, &mut buttons),
            None => {
                warn!("review: pressButton(\"{button}\") is not a known screen button (rejected)");
            }
        },
        Ok(ReviewCommand::SetAutoplay { enabled }) => autoplay.0 = enabled,
        Ok(ReviewCommand::SetTimePaused { paused }) => {
            if paused {
                virtual_time.pause();
            } else {
                virtual_time.unpause();
            }
        }
        Err(error) => warn!("review: malformed command `{raw}`: {error}"),
    }
}

/// Finds the on-screen button `target` names and sets `Interaction::Pressed`
/// on it -- the same component write a real click produces (and the exact
/// press mechanism every screen's own unit tests use), observed by the
/// screen's production handler in this same frame (see [`ReviewPlugin`]'s
/// ordering note). Missing (wrong screen) or disabled buttons are refused
/// with a `warn!`, mirroring what a real click could/couldn't do.
fn press_button(
    name: &str,
    target: ReviewButton,
    buttons: &mut Query<PressableButton, With<Button>>,
) {
    for (mut interaction, disabled, menu, creation, result, game_over, victory, shop) in
        buttons.iter_mut()
    {
        let matches = match target {
            ReviewButton::Menu(wanted) => menu == Some(&wanted),
            ReviewButton::Creation(wanted) => creation == Some(&wanted),
            ReviewButton::Result(wanted) => result == Some(&wanted),
            ReviewButton::GameOver(wanted) => game_over == Some(&wanted),
            ReviewButton::Victory(wanted) => victory == Some(&wanted),
            ReviewButton::Shop(wanted) => shop == Some(&wanted),
        };
        if !matches {
            continue;
        }
        if disabled {
            warn!("review: pressButton(\"{name}\") refused -- the button is currently disabled");
            return;
        }
        *interaction = Interaction::Pressed;
        return;
    }
    warn!("review: pressButton(\"{name}\") found no such button on the current screen");
}

/// Publishes the current [`GameState`] (its `Debug` name) every frame so the
/// harness has a semantic "which screen is this" signal. Cheap enough to do
/// unconditionally rather than only `on_event`/`is_changed`-gated: a
/// `localStorage` write is a handful of bytes and this only compiles into
/// the dedicated review build in the first place.
fn publish_current_screen(state: Res<State<GameState>>) {
    publish_screen(&format!("{:?}", state.get()));
}

/// Scripts the player's side of the duel with a fixed, deterministic policy
/// once [`ReviewAutoplay`] is enabled: Rest below the quick-strike stamina
/// cost, QuickStrike otherwise. Writes the same
/// `combat::systems::PlayerActionEvent` the HUD's action buttons write, so
/// this is deterministic *input* seeding (per the issue's seam contract),
/// never a bypass of `combat::engine::resolve_action_at_distance`. Combined
/// with a `seedCombat`-fixed [`CombatRng`] and a fixed hero preset/opponent,
/// this makes an entire duel's outcome fully reproducible -- see
/// `gold_journey_seed_wins_the_first_duel` below for the exact pinned seed
/// the `gold-journey` scenario relies on.
fn autoplay_player_turn(
    autoplay: Res<ReviewAutoplay>,
    turn: Option<Res<CombatTurn>>,
    presentation: Option<Res<CombatPresentation>>,
    stamina: Query<&Stamina, With<PlayerFighter>>,
    mut actions: MessageWriter<PlayerActionEvent>,
) {
    if !autoplay.0 {
        return;
    }
    let Some(turn) = turn else {
        return;
    };
    if turn.side != CombatSide::Player || turn.over {
        return;
    }
    if presentation
        .as_deref()
        .is_some_and(CombatPresentation::is_busy)
    {
        return;
    }
    let Ok(stamina) = stamina.single() else {
        return;
    };
    let action = if stamina.current < QUICK_STRIKE_COST {
        CombatAction::Rest
    } else {
        CombatAction::QuickStrike
    };
    actions.write(PlayerActionEvent(action));
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Reads and immediately clears [`REVIEW_COMMAND_KEY`], so a command is
/// applied at most once even if the harness's next poll happens to observe
/// the same frame.
#[cfg(target_arch = "wasm32")]
fn take_pending_command() -> Option<String> {
    let storage = local_storage()?;
    let value = storage.get_item(REVIEW_COMMAND_KEY).ok().flatten()?;
    let _ = storage.remove_item(REVIEW_COMMAND_KEY);
    Some(value)
}

#[cfg(not(target_arch = "wasm32"))]
fn take_pending_command() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn publish_screen(screen: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(REVIEW_SCREEN_KEY, screen);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn publish_screen(_screen: &str) {}

#[cfg(target_arch = "wasm32")]
fn publish_motion(json: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(REVIEW_MOTION_KEY, json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn publish_motion(_json: &str) {}

/// Clears [`REVIEW_MOTION_KEY`] so a scenario polling outside the fight (or
/// after a snapshot failed to serialize) never reads a stale motion
/// snapshot from a previous fight.
#[cfg(target_arch = "wasm32")]
fn clear_motion() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(REVIEW_MOTION_KEY);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn clear_motion() {}

#[cfg(target_arch = "wasm32")]
fn publish_palette(json: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(REVIEW_PALETTE_KEY, json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn publish_palette(_json: &str) {}

/// Clears [`REVIEW_PALETTE_KEY`] so a scenario polling outside the fight (or
/// with zero buttons spawned) never reads a stale palette snapshot from a
/// previous fight.
#[cfg(target_arch = "wasm32")]
fn clear_palette() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(REVIEW_PALETTE_KEY);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn clear_palette() {}

#[cfg(target_arch = "wasm32")]
fn publish_theme(json: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(REVIEW_THEME_KEY, json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn publish_theme(_json: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Affine2;

    // --- `fight-palette-desktop` (#189) geometry helper ---

    #[test]
    fn logical_node_rect_centers_the_computed_node_at_the_transform() {
        let transform = UiGlobalTransform::from(Affine2::from_translation(Vec2::new(100.0, 50.0)));
        let node = ComputedNode {
            size: Vec2::new(40.0, 20.0),
            inverse_scale_factor: 1.0,
            ..Default::default()
        };
        let rect = logical_node_rect(&transform, &node);
        assert_eq!(rect, Rect::new(80.0, 40.0, 120.0, 60.0));
    }

    #[test]
    fn logical_node_rect_scales_physical_pixels_down_to_logical() {
        // A 2x DPR node: 80x40 physical pixels is 40x20 logical, centered on
        // a transform whose translation is itself in physical pixels.
        let transform = UiGlobalTransform::from(Affine2::from_translation(Vec2::new(200.0, 100.0)));
        let node = ComputedNode {
            size: Vec2::new(80.0, 40.0),
            inverse_scale_factor: 0.5,
            ..Default::default()
        };
        let rect = logical_node_rect(&transform, &node);
        assert_eq!(rect, Rect::new(80.0, 40.0, 120.0, 60.0));
    }

    #[test]
    fn seed_combat_command_parses() {
        assert_eq!(
            serde_json::from_str::<ReviewCommand>(r#"{"cmd":"seedCombat","seed":1234}"#).unwrap(),
            ReviewCommand::SeedCombat { seed: 1234 }
        );
    }

    #[test]
    fn select_preset_command_parses() {
        assert_eq!(
            serde_json::from_str::<ReviewCommand>(r#"{"cmd":"selectPreset","preset":"Voinicul"}"#)
                .unwrap(),
            ReviewCommand::SelectPreset {
                preset: "Voinicul".to_string()
            }
        );
    }

    #[test]
    fn press_button_command_parses() {
        assert_eq!(
            serde_json::from_str::<ReviewCommand>(
                r#"{"cmd":"pressButton","button":"ConfirmHero"}"#
            )
            .unwrap(),
            ReviewCommand::PressButton {
                button: "ConfirmHero".to_string()
            }
        );
    }

    #[test]
    fn set_autoplay_command_parses() {
        assert_eq!(
            serde_json::from_str::<ReviewCommand>(r#"{"cmd":"setAutoplay","enabled":true}"#)
                .unwrap(),
            ReviewCommand::SetAutoplay { enabled: true }
        );
    }

    #[test]
    fn set_time_paused_command_parses() {
        assert_eq!(
            serde_json::from_str::<ReviewCommand>(r#"{"cmd":"setTimePaused","paused":true}"#)
                .unwrap(),
            ReviewCommand::SetTimePaused { paused: true }
        );
    }

    #[test]
    fn malformed_command_is_a_parse_error_not_a_panic() {
        assert!(serde_json::from_str::<ReviewCommand>("not json").is_err());
        assert!(serde_json::from_str::<ReviewCommand>(r#"{"cmd":"bogus"}"#).is_err());
    }

    #[test]
    fn parse_preset_matches_every_hero_preset_by_its_display_name() {
        for preset in HeroPreset::ALL {
            assert_eq!(parse_preset(preset.name()), Some(preset));
        }
        assert_eq!(parse_preset("Not A Hero"), None);
    }

    #[test]
    fn parse_button_covers_every_player_facing_navigation_button() {
        for (name, expected) in [
            ("NewGame", ReviewButton::Menu(MenuAction::NewGame)),
            ("Continue", ReviewButton::Menu(MenuAction::Continue)),
            (
                "ConfirmHero",
                ReviewButton::Creation(CreationAction::Confirm),
            ),
            ("CreationBack", ReviewButton::Creation(CreationAction::Back)),
            ("GoToShop", ReviewButton::Result(ResultAction::GoToShop)),
            ("NextFight", ReviewButton::Result(ResultAction::NextFight)),
            (
                "GameOverBackToMenu",
                ReviewButton::GameOver(GameOverAction::BackToMenu),
            ),
            (
                "VictoryNextLap",
                ReviewButton::Victory(VictoryAction::NextLap),
            ),
            (
                "VictoryBackToMenu",
                ReviewButton::Victory(VictoryAction::BackToMenu),
            ),
            ("BackToArena", ReviewButton::Shop(ShopAction::BackToArena)),
        ] {
            assert_eq!(parse_button(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn parse_button_rejects_unknown_and_non_navigation_names() {
        // The automated combat-outcome routes have no button and must stay
        // unreachable from the seam; in-screen editors (attribute steppers,
        // shop purchases) are seeded through dedicated commands instead.
        for name in [
            "ResolveVictory",
            "ResolveDefeat",
            "RunWon",
            "BuyItem",
            "IncreasePutere",
            "NotAButton",
        ] {
            assert_eq!(parse_button(name), None, "{name} must be rejected");
        }
    }

    /// End-to-end press through a real (headless) app: `pressButton
    /// NewGame` presses the actual menu button, whose production handler
    /// resets the run and emits `StartNewGame`, and the flow table routes to
    /// creation -- proving the seam drives the same path as a player click.
    #[test]
    fn press_button_drives_the_production_menu_handler() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::state::app::StatesPlugin;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            crate::core::CorePlugin,
            crate::flow::FlowPlugin,
            crate::menu::MenuPlugin,
            ReviewPlugin,
        ));
        app.update(); // headless Loading fall-through queues MainMenu (#114)
        app.update(); // transition applies; menu spawns
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu
        );

        // Native builds have no localStorage; feed the command through the
        // same dispatch the wasm poll path uses.
        let pressed = app
            .world_mut()
            .run_system_once(move |mut buttons: Query<PressableButton, With<Button>>| {
                press_button(
                    "NewGame",
                    ReviewButton::Menu(MenuAction::NewGame),
                    &mut buttons,
                );
            })
            .is_ok();
        assert!(pressed, "press system runs");
        app.update(); // handler observes Pressed, emits StartNewGame
        app.update(); // flow applies the transition
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::CharacterCreation,
            "the pressed menu button routes menu -> creation through the production handler"
        );
    }

    // --- Determinism pin for the `gold-journey` scenario ---
    //
    // Simulates the exact duel `gold_journey.rs` drives (the `Voinicul`
    // preset's attributes + starter equipment vs. the ladder's first
    // opponent, `Hoț de codru`), using the pure `engine`/`ai` functions
    // directly (no ECS/World needed -- mirrors
    // `combat::ai::tests::duels_against_the_strigoi_are_winnable_and_losable`),
    // driving the player side with the *exact* policy `autoplay_player_turn`
    // above uses. This pins that `gold_journey::GOLD_JOURNEY_SEED` (kept in
    // sync with the constant of the same name in
    // `xtask/src/web_smoke/gold_journey.rs` -- see that module's docs for why
    // the two crates can't share the literal) reaches a player victory
    // within a small, fixed number of turns -- the actual browser scenario's
    // determinism guarantee rests on this exact simulation being
    // reproducible for a fixed seed, which `combat::engine`/`combat::ai`'s
    // own test suites already establish independently.
    mod gold_journey_seed {
        use crate::character::{Attributes, stats};
        use crate::combat::ai::{self, AiProfile};
        use crate::combat::engine::{self, CombatAction, CombatEvent, DuelDistance, FighterState};
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        /// Kept equal to `xtask::web_smoke::gold_journey::GOLD_JOURNEY_SEED`.
        const GOLD_JOURNEY_SEED: u64 = 20;
        const MAX_TURNS: usize = 60;

        /// `HeroPreset::Voinicul`'s attributes, with its starter loadout's
        /// flat bonuses aggregated in directly (`BataCiobaneasca` damage 3,
        /// `ScutDeLemn` armor 1) -- mirrors how `combat::systems::snapshot`
        /// aggregates `Equipment` onto a `FighterState` in the real ECS path.
        fn player() -> FighterState {
            let mut fighter = FighterState::new(Attributes {
                putere: 4,
                agilitate: 3,
                vitalitate: 4,
                noroc: 3,
            });
            fighter.damage_bonus = 3;
            fighter.armor = 1;
            fighter
        }

        /// The ladder's first opponent, `Hoț de codru` (`roster::LADDER[0]`):
        /// unarmed, aggression 0.25.
        fn opponent() -> (FighterState, AiProfile) {
            (
                FighterState::new(Attributes {
                    putere: 2,
                    agilitate: 2,
                    vitalitate: 2,
                    noroc: 1,
                }),
                AiProfile { aggression: 0.25 },
            )
        }

        /// The exact player policy `autoplay_player_turn` scripts: Rest below
        /// the quick-strike cost, QuickStrike otherwise.
        fn player_policy(me: &FighterState) -> CombatAction {
            if me.stamina < engine::QUICK_STRIKE_COST {
                CombatAction::Rest
            } else {
                CombatAction::QuickStrike
            }
        }

        #[test]
        fn gold_journey_seed_wins_the_first_duel() {
            let mut player = player();
            let (mut enemy, profile) = opponent();
            let mut rng = ChaCha8Rng::seed_from_u64(GOLD_JOURNEY_SEED);
            let mut distance = DuelDistance::starting();
            assert!(
                engine::player_acts_first(&player.attributes, &enemy.attributes),
                "Voinicul (agilitate 3) must open the round against Hoț de codru (agilitate 2)"
            );

            for turn in 0..MAX_TURNS {
                let action = player_policy(&player);
                let events = engine::resolve_action_at_distance(
                    &mut player,
                    &mut enemy,
                    action,
                    &mut distance,
                    &mut rng,
                );
                if events.contains(&CombatEvent::Defeated) {
                    assert_eq!(
                        enemy.hp, 0,
                        "turn {turn}: the player's strike ends the duel"
                    );
                    return;
                }
                let action =
                    ai::choose_action_at_distance(&enemy, &player, &profile, distance, &mut rng);
                let events = engine::resolve_action_at_distance(
                    &mut enemy,
                    &mut player,
                    action,
                    &mut distance,
                    &mut rng,
                );
                assert!(
                    !events.contains(&CombatEvent::Defeated),
                    "turn {turn}: the gold-journey seed must not lose the first duel"
                );
            }
            panic!(
                "seed {GOLD_JOURNEY_SEED} did not finish the duel within {MAX_TURNS} turns \
                 (enemy hp {}, player hp {})",
                enemy.hp, player.hp
            );
        }

        /// Sanity check on the derived numbers this pin relies on.
        #[test]
        fn matchup_numbers_match_the_preset_and_ladder_data() {
            let player = player();
            assert_eq!(stats::max_hp(&player.attributes), 90);
            assert_eq!(
                stats::base_damage(&player.attributes) + player.damage_bonus,
                9
            );
            let (enemy, _) = opponent();
            assert_eq!(stats::max_hp(&enemy.attributes), 70);
        }
    }
}
