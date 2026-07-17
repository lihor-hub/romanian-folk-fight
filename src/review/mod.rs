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
//!   - `pressActionCategory`: sets `Interaction::Pressed` on the named phone
//!     category's real [`crate::combat::action_palette::CategoryButton`]
//!     entity (#199) -- the same production toggle a tap produces, driving
//!     [`crate::combat::action_palette::handle_category_buttons`] exactly
//!     like `pressButton` drives a screen's navigation handler.
//!   - `setTimePaused`: pauses/unpauses Bevy's `Time<Virtual>` so the
//!     harness can capture a byte-stable screenshot on screens with
//!     continuous idle animation; see [`ReviewCommand::SetTimePaused`].
//!   - `advanceTime`: jumps `Time<Virtual>` forward by a fixed number of
//!     seconds in one step (not a per-frame tick), sent right before
//!     `setTimePaused` so any bounded, time-driven reveal animation is
//!     unambiguously finished before the clock freezes for a screenshot
//!     (#272); see [`advance_virtual_time`]'s doc comment.
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

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::arena::fx::ParallaxLayer;
use crate::arena::{ENEMY_ANCHOR, PLAYER_ANCHOR};
use crate::character::{EnemyFighter, PlayerFighter, Stamina};
use crate::combat::action_palette::{
    ActionButton, ActionCostOrReason, CategoryButton, PhonePaletteState,
};
use crate::combat::actions::{self, ActionCategory};
use crate::combat::engine::QUICK_STRIKE_COST;
use crate::combat::pause::PauseAction;
use crate::combat::systems::{CombatPresentation, PlayerActionEvent};
use crate::combat::{CombatAction, CombatRng, CombatSide, CombatTurn};
use crate::core::{GameState, LetterboxRect, ViewportInfo, WorldCamera};
use crate::creation::{CharacterDraft, CreationAction, HeroChoice, HeroPreset};
use crate::items::ItemId;
use crate::menu::{DisabledButton, MenuAction};
use crate::progression::result_ui::{GameOverAction, ResultAction};
use crate::progression::victory_ui::VictoryAction;
use crate::settings::AccessibilityPreferences;
use crate::shop::ShopAction;
use crate::theme::Palette;
use crate::ui_widgets::focus::Focusable;

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
/// `localStorage` key this seam publishes an [`AccessibilitySnapshot`] to
/// every frame -- added for #216's cross-screen `keyboard-accessibility`,
/// `touch-targets`, and `zoom-200` browser scenarios. Unlike
/// [`REVIEW_PALETTE_KEY`] (fight-HUD-only), this is published unconditionally
/// like [`REVIEW_THEME_KEY`]: #216 rolls [`Focusable`] out to every screen,
/// not just the fight HUD, so there is no single screen this is scoped to
/// and no corresponding `clear_accessibility`. See [`REVIEW_COMMAND_KEY`]'s
/// doc comment for why the key itself is duplicated as a plain string on the
/// `xtask` side.
pub const REVIEW_ACCESSIBILITY_KEY: &str = "rff_review_a11y_v1";

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
    /// Sets `Interaction::Pressed` on the named phone category's real
    /// [`CategoryButton`] entity (#199) — the same production toggle a tap
    /// produces, used by the `fight-palette-phone` browser scenario to open
    /// (or close/switch) a category without a synthetic pointer event.
    /// `category` is one of [`crate::combat::actions::category_id`]'s kebab-
    /// case ids (e.g. `"strikes"`).
    PressActionCategory {
        category: String,
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
    /// Jumps Bevy's `Time<Virtual>` forward by `seconds` in a single step
    /// (`Time::advance_by`, not a per-frame tick) -- see
    /// [`advance_virtual_time`]'s doc comment for the determinism problem
    /// this solves (#272). Sent by the harness right before `SetTimePaused`
    /// on every checkpoint, so any bounded, time-driven reveal animation
    /// (e.g. a result-screen count-up) is unambiguously past its terminal
    /// frame before the clock freezes for a screenshot.
    AdvanceTime {
        seconds: f32,
    },
}

/// Applies [`ReviewCommand::AdvanceTime`]: jumps `virtual_time` forward by
/// `seconds` in one call to `Time::advance_by` -- not several small per-frame
/// ticks -- so every system reading `Time<Virtual>` observes a `delta_secs()`
/// far larger than any plausible in-game animation duration on its very next
/// `Update`, rather than the harness waiting real frames for the clock to
/// accumulate that much elapsed time on its own.
///
/// # Why this fixes #272
///
/// Before this command existed, the harness's only way to confirm a screen
/// had "settled" before freezing the clock (`SetTimePaused`) was #168's
/// byte-identical-screenshot streak: wait for a few consecutive rendered
/// frames to come out pixel-identical, then freeze. That heuristic silently
/// assumes identical consecutive frames only happen once nothing is still
/// animating -- but a smoothly *quantized* value (e.g. a count-up rounded to
/// the nearest whole galbeni) can render the *same* rounded pixels across
/// several consecutive frames while still mid-animation, satisfying the
/// streak by coincidence at whatever fraction of the animation's duration
/// the harness happened to sample -- exactly the "captured at ~14%/~65%
/// progress" nondeterminism #272 reports. Explicitly fast-forwarding the
/// clock past any plausible animation duration *before* relying on that
/// streak removes the coincidence: by the time frames are compared, the
/// animation is guaranteed finished, not just quantized-still for a moment.
///
/// A negative `seconds` clamps to zero rather than panicking (`Duration`
/// cannot represent a negative amount and time must never move backwards).
fn advance_virtual_time(virtual_time: &mut Time<Virtual>, seconds: f32) {
    virtual_time.advance_by(std::time::Duration::from_secs_f32(seconds.max(0.0)));
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
            // #213: publish_palette_state's focus facts read this; idempotent
            // with `ui_widgets::focus::FocusNavigationPlugin`'s own
            // registration (added by `CombatPlugin`).
            .init_resource::<InputFocus>()
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
                    // write must land within the same `Update` pass. The same
                    // reasoning requires ordering before the phone palette's
                    // category-toggle handler (#199), which is not part of
                    // `FlowIntentEmission` (it toggles in-screen disclosure
                    // state, never a flow intent).
                    poll_review_commands
                        .before(crate::flow::FlowIntentEmission)
                        .before(crate::combat::action_palette::handle_category_buttons),
                    publish_current_screen,
                    publish_motion_state,
                    publish_palette_state,
                    publish_theme_state,
                    publish_accessibility_state,
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
///
/// `phone` (#199) extends this with the phone category-disclosure facts the
/// `fight-palette-phone` scenario needs, populated only when
/// [`ViewportInfo::is_mobile`] is true -- `None` on desktop, since desktop
/// never groups into categories. A desktop-only consumer that only ever
/// deserializes `button_count`/`fits` (like `fight_palette_desktop.rs`'s
/// mirrored struct on the `xtask` side) is unaffected: `serde` ignores the
/// extra field by default.
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct PaletteSnapshot {
    /// How many action buttons currently exist (spawned = rendered; #189's
    /// palette never despawns/hides a button to make it fit). On phone this
    /// is the *open* category's button count (0 while closed), not the full
    /// registered-action count -- see `phone` for the category controls.
    button_count: usize,
    /// Whether every currently visible interactive control -- action
    /// buttons plus (on phone) category buttons -- lies entirely within the
    /// letterboxed stage rect. `false` (or nothing visible at all) means the
    /// scenario must fail: an overflowing or clipped palette.
    fits: bool,
    /// Phone category-disclosure facts (#199), or `None` on desktop.
    phone: Option<PhonePaletteSnapshot>,
    /// Descriptor-driven keyboard/gamepad focus facts (#213), or `None` when
    /// nothing currently has focus (e.g. before the player has pressed Tab).
    focus: Option<FocusSnapshot>,
}

/// Everything the `fight-palette-accessible` scenario (#213) needs to assert
/// keyboard/gamepad focus beyond what a headless test can prove: which
/// control [`bevy::input_focus::InputFocus`] actually names after a *real*
/// CDP-dispatched key event lands the game's real winit keyboard pipeline,
/// whether that control's disabled reason (if any) is a real rendered
/// sentence, and whether the visible focus marker
/// ([`crate::ui_widgets::focus`]) is actually showing. Published as part of
/// [`PaletteSnapshot`] every frame the fight HUD's action bar is up, exactly
/// like [`PhonePaletteSnapshot`] is.
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct FocusSnapshot {
    /// The stable id of the currently focused control: a
    /// [`crate::combat::actions::ActionId`] for an action button, or
    /// [`crate::combat::actions::category_id`]'s id for a phone category
    /// button.
    focused_id: String,
    /// Whether the focused control is a phone category button (`false` for
    /// an action button -- categories and actions never share an id
    /// namespace, but this disambiguates without the scenario needing to
    /// know every category id by heart).
    focused_is_category: bool,
    /// Whether the focused action button is currently disabled (always
    /// `false` for a category button, which is never disabled).
    focused_is_disabled: bool,
    /// The action button's shown cost/disabled-reason text (the same
    /// [`ActionCostOrReason`] slot the palette always renders) -- `None` for
    /// a category button.
    focused_reason_text: Option<String>,
    /// Whether the focused control currently renders the visible gold focus
    /// marker (a non-transparent `Outline`) -- read from the live component,
    /// not a screenshot pixel probe.
    focus_marker_visible: bool,
}

/// One currently-visible interactive control's on-screen box, in logical
/// (CSS) pixels -- the unit both the 44px touch-target floor and
/// [`LetterboxRect`] are expressed in.
#[derive(serde::Serialize, Debug, Clone, Copy, PartialEq)]
struct TargetRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl TargetRect {
    fn from_rect(rect: Rect) -> Self {
        Self {
            x: rect.min.x,
            y: rect.min.y,
            width: rect.width(),
            height: rect.height(),
        }
    }
}

/// Everything the `fight-palette-phone` scenario (#199) needs to assert the
/// category-disclosure palette beyond what [`PaletteSnapshot`]'s top-level
/// fields already cover: how many primary category controls are visible
/// (must never exceed four), which one (if any) is currently open, the exact
/// on-screen box of every currently visible target (category buttons plus,
/// while open, that category's action buttons), whether they all fit inside
/// the stage, and the smallest dimension across all of them (the 44px CSS
/// touch-target floor the issue requires).
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct PhonePaletteSnapshot {
    visible_category_count: usize,
    /// [`crate::combat::actions::category_id`] of the open category, or
    /// `None` while closed.
    open_category: Option<String>,
    /// The stable [`crate::combat::actions::ActionId`]s of the currently
    /// visible action buttons (the open category's members), sorted -- so
    /// the phone scenario can assert "registered actions only" by exact id,
    /// not just count.
    open_action_ids: Vec<String>,
    targets: Vec<TargetRect>,
    fits_in_stage: bool,
    /// `min(width, height)` across every entry in `targets`; `0.0` if
    /// `targets` is empty (nothing currently visible to measure).
    min_target_size: f32,
    /// Whether any visible palette control's box intersects a fighter
    /// status panel ([`crate::combat::hud::FighterPanelRoot`]) -- `true`
    /// means the palette covers required fighter/status information, which
    /// #199 forbids.
    overlaps_status_panels: bool,
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

/// The data [`focus_snapshot`] needs to describe whichever
/// [`bevy::input_focus::InputFocus`]-named entity is an action button:
/// its descriptor id, whether it is disabled, its child nodes (to find the
/// reason text among them), and its current `Outline` (if any).
type FocusActionButton<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActionButton,
        Has<DisabledButton>,
        &'static Children,
        Option<&'static Outline>,
    ),
>;

/// Builds [`FocusSnapshot`] from whichever entity [`InputFocus`] currently
/// names, or `None` if nothing does. Checked against the action-button query
/// first, then the category-button query -- the two are mutually exclusive
/// (`ActionButton`/`CategoryButton` never coexist on one entity), so at most
/// one ever matches.
fn focus_snapshot(
    input_focus: &InputFocus,
    action_buttons: &FocusActionButton,
    category_buttons: &Query<(Entity, &CategoryButton, Option<&Outline>)>,
    reason_texts: &Query<&Text, With<ActionCostOrReason>>,
) -> Option<FocusSnapshot> {
    let focused = input_focus.get()?;
    if let Ok((_, button, disabled, children, outline)) = action_buttons.get(focused) {
        let reason_text = children
            .iter()
            .find_map(|child| reason_texts.get(child).ok())
            .map(|text| text.0.clone());
        return Some(FocusSnapshot {
            focused_id: button.id.to_string(),
            focused_is_category: false,
            focused_is_disabled: disabled,
            focused_reason_text: reason_text,
            focus_marker_visible: outline.is_some_and(|outline| outline.color != Color::NONE),
        });
    }
    let (_, button, outline) = category_buttons.get(focused).ok()?;
    Some(FocusSnapshot {
        focused_id: actions::category_id(button.category).to_string(),
        focused_is_category: true,
        focused_is_disabled: false,
        focused_reason_text: None,
        focus_marker_visible: outline.is_some_and(|outline| outline.color != Color::NONE),
    })
}

/// Publishes a [`PaletteSnapshot`] every frame at least one action or
/// category button exists (clears the key otherwise, e.g. outside the fight
/// screen, so a scenario can't mistake a stale snapshot from a previous
/// fight for the current one).
#[allow(clippy::too_many_arguments)]
fn publish_palette_state(
    letterbox: Option<Res<LetterboxRect>>,
    viewport: Res<ViewportInfo>,
    phone_state: Option<Res<PhonePaletteState>>,
    input_focus: Res<InputFocus>,
    buttons: Query<(&ActionButton, &UiGlobalTransform, &ComputedNode)>,
    categories: Query<(&UiGlobalTransform, &ComputedNode), With<CategoryButton>>,
    status_panels: Query<
        (&UiGlobalTransform, &ComputedNode),
        With<crate::combat::hud::FighterPanelRoot>,
    >,
    focus_action_buttons: FocusActionButton,
    focus_category_buttons: Query<(Entity, &CategoryButton, Option<&Outline>)>,
    reason_texts: Query<&Text, With<ActionCostOrReason>>,
) {
    let Some(letterbox) = letterbox else {
        clear_palette();
        return;
    };
    let stage = Rect::from_corners(letterbox.position, letterbox.position + letterbox.size);

    let button_count = buttons.iter().count();
    let mut all_rects: Vec<Rect> = buttons
        .iter()
        .map(|(_, transform, node)| logical_node_rect(transform, node))
        .collect();
    let category_rects: Vec<Rect> = categories
        .iter()
        .map(|(transform, node)| logical_node_rect(transform, node))
        .collect();
    all_rects.extend(category_rects.iter().copied());

    if all_rects.is_empty() {
        clear_palette();
        return;
    }

    let mut extent = all_rects[0];
    for rect in &all_rects[1..] {
        extent = extent.union(*rect);
    }
    let fits = stage.contains(extent.min) && stage.contains(extent.max);

    let phone = viewport.is_mobile.then(|| {
        let open_category = phone_state
            .as_deref()
            .and_then(|state| state.open)
            .map(actions::category_id)
            .map(str::to_string);
        let mut open_action_ids: Vec<String> = buttons
            .iter()
            .map(|(button, _, _)| button.id.to_string())
            .collect();
        open_action_ids.sort_unstable();
        let min_target_size = all_rects
            .iter()
            .map(|r| r.width().min(r.height()))
            .fold(f32::INFINITY, f32::min);
        let panel_rects: Vec<Rect> = status_panels
            .iter()
            .map(|(transform, node)| logical_node_rect(transform, node))
            .collect();
        let overlaps_status_panels = all_rects.iter().any(|target| {
            panel_rects
                .iter()
                .any(|panel| !panel.intersect(*target).is_empty())
        });
        PhonePaletteSnapshot {
            visible_category_count: category_rects.len(),
            open_category,
            open_action_ids,
            targets: all_rects
                .iter()
                .copied()
                .map(TargetRect::from_rect)
                .collect(),
            fits_in_stage: fits,
            min_target_size: if min_target_size.is_finite() {
                min_target_size
            } else {
                0.0
            },
            overlaps_status_panels,
        }
    });

    let focus = focus_snapshot(
        &input_focus,
        &focus_action_buttons,
        &focus_category_buttons,
        &reason_texts,
    );

    let snapshot = PaletteSnapshot {
        button_count,
        fits,
        phone,
        focus,
    };
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

/// Everything the #216 cross-screen browser scenarios need that
/// [`PaletteSnapshot`] (fight-HUD-only) cannot provide: which control is
/// currently focused (by its own rendered label, so the scenario can assert
/// against the exact Romanian text a player sees, the same way it already
/// reads button labels through the DOM-less canvas), whether the gold focus
/// marker is actually rendered, and every currently-visible [`Focusable`]
/// control's on-screen box in logical (CSS) pixels -- the same unit the 44px
/// touch-target floor and the window's own `innerWidth`/`innerHeight` are
/// expressed in, so a scenario can assert both "nothing is smaller than
/// 44x44" (`touch-targets`) and "nothing sits outside the viewport"
/// (`zoom-200`) without any new native-side math.
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct AccessibilitySnapshot {
    /// A stable identifier for the currently focused entity (its `Debug`
    /// representation, e.g. `"16v0"`), or `None` if nothing is focused.
    /// Several controls across a screen can share the exact same rendered
    /// `focused_label` (both the music and the SFX steppers' decrease
    /// buttons render literally "-"), so a scenario doing cycle detection
    /// (walking `ArrowRight` until focus returns to where it started) needs
    /// an identifier `focused_label` alone cannot provide.
    focused_entity: Option<String>,
    /// The focused control's own rendered label (its first `Text` child),
    /// or `None` if nothing is focused or the focused control has no direct
    /// `Text` child (every button built through this codebase's shared
    /// button helpers or the palette's own bundles has exactly one).
    focused_label: Option<String>,
    /// Whether the focused control currently renders the visible gold focus
    /// marker (a non-transparent `Outline`) -- read from the live
    /// component, not a screenshot pixel probe. `false` when nothing is
    /// focused.
    focus_marker_visible: bool,
    /// The focused control's *current* on-screen box (post-scroll: the
    /// shared widget's `scroll_focused_into_view` runs before this snapshot
    /// is published, so this is where the control actually renders after
    /// any in-UI scrolling settled). The `zoom-200` scenario's clipping
    /// gate reads this per tab-stop: "playable at 200% zoom" means every
    /// control is *visible when focused*, not that every control of a
    /// designed-to-scroll screen fits one viewport simultaneously.
    focused_rect: Option<TargetRect>,
    /// Every currently-visible `Focusable` control's on-screen box.
    targets: Vec<TargetRect>,
    /// `min(width, height)` across every entry in `targets`; `0.0` if
    /// `targets` is empty.
    min_target_size: f32,
}

/// The first `Text` among `entity`'s direct children, if any -- every button
/// this codebase's shared helpers (`ui_widgets::{button_bundle, wide_button,
/// small_button}`) and the screen-local equivalents (`menu::menu_button`,
/// `combat::pause`'s overlay buttons) build carries its label exactly one
/// level down, the same shape [`focus_snapshot`] already relies on for the
/// palette's disabled-reason text.
fn direct_child_label(
    entity: Entity,
    children_query: &Query<&Children>,
    texts: &Query<&Text>,
) -> Option<String> {
    let children = children_query.get(entity).ok()?;
    children
        .iter()
        .find_map(|child| texts.get(child).ok())
        .map(|text| text.0.clone())
}

/// Publishes an [`AccessibilitySnapshot`] every frame (unconditionally --
/// see [`REVIEW_ACCESSIBILITY_KEY`]'s doc comment for why there is no
/// per-screen gate or corresponding `clear`).
fn publish_accessibility_state(
    input_focus: Res<InputFocus>,
    focusables: Query<(Entity, &UiGlobalTransform, &ComputedNode), With<Focusable>>,
    outlines: Query<&Outline>,
    children_query: Query<&Children>,
    texts: Query<&Text>,
) {
    let targets: Vec<TargetRect> = focusables
        .iter()
        .map(|(_, transform, node)| TargetRect::from_rect(logical_node_rect(transform, node)))
        .collect();
    let min_target_size = targets
        .iter()
        .map(|rect| rect.width.min(rect.height))
        .fold(f32::INFINITY, f32::min);

    let focused = input_focus.get();
    let focused_label =
        focused.and_then(|entity| direct_child_label(entity, &children_query, &texts));
    let focus_marker_visible = focused
        .and_then(|entity| outlines.get(entity).ok())
        .is_some_and(|outline| outline.color != Color::NONE);
    let focused_rect = focused.and_then(|entity| {
        focusables
            .get(entity)
            .ok()
            .map(|(_, transform, node)| TargetRect::from_rect(logical_node_rect(transform, node)))
    });

    let snapshot = AccessibilitySnapshot {
        focused_entity: focused.map(|entity| format!("{entity:?}")),
        focused_label,
        focus_marker_visible,
        focused_rect,
        targets,
        min_target_size: if min_target_size.is_finite() {
            min_target_size
        } else {
            0.0
        },
    };
    if let Ok(json) = serde_json::to_string(&snapshot) {
        publish_accessibility(&json);
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
/// journey screens (plus game-over/victory, the pause overlay's forfeit
/// action, and shop purchases for the save/abandon browser scenarios --
/// #217); in-screen editing buttons that aren't navigation or a checkpoint in
/// their own right (attribute steppers, ...) are deliberately not exposed --
/// in-screen state is seeded through dedicated commands like `selectPreset`
/// instead.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ReviewButton {
    Menu(MenuAction),
    Creation(CreationAction),
    Result(ResultAction),
    GameOver(GameOverAction),
    Victory(VictoryAction),
    Shop(ShopAction),
    /// The paused-fight overlay's own actions (#217): only **Abandonează**
    /// is exercised by a browser scenario today (`abandon-forfeit`), but
    /// **Continuă lupta**/**Setări** are exposed the same way for symmetry
    /// with every other screen's full action set.
    Pause(PauseAction),
}

/// Maps a `pressButton` command's `button` field to the screen button it
/// presses. An unrecognized name returns `None`, which
/// [`poll_review_commands`] logs and drops. `ShopItem:<name>` (#217) is a
/// dynamic family rather than a fixed variant, since it addresses one of
/// every catalog item's own buy/equip button by [`ItemId`]'s `Debug` name
/// (the same stable name [`crate::save::snapshot`] uses for a save's
/// `owned_items`/`equipped` lists) -- checked before the fixed-name match so
/// a name never collides with one of the exact strings below.
fn parse_button(name: &str) -> Option<ReviewButton> {
    if let Some(item_name) = name.strip_prefix("ShopItem:") {
        return ItemId::ALL
            .into_iter()
            .find(|id| format!("{id:?}") == item_name)
            .map(|id| ReviewButton::Shop(ShopAction::Item(id)));
    }
    match name {
        "NewGame" => Some(ReviewButton::Menu(MenuAction::NewGame)),
        "Continue" => Some(ReviewButton::Menu(MenuAction::Continue)),
        // #201: the Romanian recovery action shown in place of Continuă
        // when the stored run snapshot is present but unusable -- lets the
        // `corrupt-save-recovery` browser scenario trigger recovery
        // deterministically instead of guessing pixel coordinates.
        "ClearCorruptSave" => Some(ReviewButton::Menu(MenuAction::ClearCorruptSave)),
        "ConfirmHero" => Some(ReviewButton::Creation(CreationAction::Confirm)),
        "CreationBack" => Some(ReviewButton::Creation(CreationAction::Back)),
        "GoToShop" => Some(ReviewButton::Result(ResultAction::GoToShop)),
        "NextFight" => Some(ReviewButton::Result(ResultAction::NextFight)),
        "GameOverBackToMenu" => Some(ReviewButton::GameOver(GameOverAction::BackToMenu)),
        "VictoryNextLap" => Some(ReviewButton::Victory(VictoryAction::NextLap)),
        "VictoryBackToMenu" => Some(ReviewButton::Victory(VictoryAction::BackToMenu)),
        "BackToArena" => Some(ReviewButton::Shop(ShopAction::BackToArena)),
        // #217: the paused-fight overlay's actions -- see `ReviewButton::Pause`.
        "PauseResume" => Some(ReviewButton::Pause(PauseAction::Resume)),
        "PauseSettings" => Some(ReviewButton::Pause(PauseAction::Settings)),
        "PauseAbandon" => Some(ReviewButton::Pause(PauseAction::Abandon)),
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
    Option<&'static PauseAction>,
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
    mut buttons: Query<PressableButton, (With<Button>, Without<CategoryButton>)>,
    mut categories: Query<(&mut Interaction, &CategoryButton), With<Button>>,
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
        Ok(ReviewCommand::PressActionCategory { category }) => {
            match actions::parse_category_id(&category) {
                Some(target) => {
                    if !press_category_button(target, &mut categories) {
                        warn!(
                            "review: pressActionCategory(\"{category}\") found no such category \
                         button on the current screen"
                        );
                    }
                }
                None => {
                    warn!(
                        "review: pressActionCategory(\"{category}\") is not a known action category"
                    );
                }
            }
        }
        Ok(ReviewCommand::SetAutoplay { enabled }) => autoplay.0 = enabled,
        Ok(ReviewCommand::SetTimePaused { paused }) => {
            if paused {
                virtual_time.pause();
            } else {
                virtual_time.unpause();
            }
        }
        Ok(ReviewCommand::AdvanceTime { seconds }) => {
            advance_virtual_time(&mut virtual_time, seconds);
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
    buttons: &mut Query<PressableButton, (With<Button>, Without<CategoryButton>)>,
) {
    for (mut interaction, disabled, menu, creation, result, game_over, victory, shop, pause) in
        buttons.iter_mut()
    {
        let matches = match target {
            ReviewButton::Menu(wanted) => menu == Some(&wanted),
            ReviewButton::Creation(wanted) => creation == Some(&wanted),
            ReviewButton::Result(wanted) => result == Some(&wanted),
            ReviewButton::GameOver(wanted) => game_over == Some(&wanted),
            ReviewButton::Victory(wanted) => victory == Some(&wanted),
            ReviewButton::Shop(wanted) => shop == Some(&wanted),
            ReviewButton::Pause(wanted) => pause == Some(&wanted),
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

/// Finds `category`'s real [`CategoryButton`] entity and sets
/// `Interaction::Pressed` on it (#199) — the same production toggle a tap
/// produces, observed by [`crate::combat::action_palette::handle_category_buttons`]
/// this same frame (see [`ReviewPlugin`]'s ordering note). Returns whether a
/// matching button was found (categories are never disabled, so unlike
/// [`press_button`] there is no separate "refused" outcome).
fn press_category_button(
    category: ActionCategory,
    buttons: &mut Query<(&mut Interaction, &CategoryButton), With<Button>>,
) -> bool {
    for (mut interaction, button) in buttons.iter_mut() {
        if button.category == category {
            *interaction = Interaction::Pressed;
            return true;
        }
    }
    false
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

#[cfg(target_arch = "wasm32")]
fn publish_accessibility(json: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(REVIEW_ACCESSIBILITY_KEY, json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn publish_accessibility(_json: &str) {}

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
    fn target_rect_from_rect_converts_min_corner_and_extent() {
        let rect = Rect::from_center_size(Vec2::new(100.0, 50.0), Vec2::new(40.0, 20.0));
        let target = TargetRect::from_rect(rect);
        assert_eq!(
            target,
            TargetRect {
                x: 80.0,
                y: 40.0,
                width: 40.0,
                height: 20.0,
            }
        );
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
    fn press_action_category_command_parses() {
        assert_eq!(
            serde_json::from_str::<ReviewCommand>(
                r#"{"cmd":"pressActionCategory","category":"strikes"}"#
            )
            .unwrap(),
            ReviewCommand::PressActionCategory {
                category: "strikes".to_string()
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
    fn advance_time_command_parses() {
        assert_eq!(
            serde_json::from_str::<ReviewCommand>(r#"{"cmd":"advanceTime","seconds":5.0}"#)
                .unwrap(),
            ReviewCommand::AdvanceTime { seconds: 5.0 }
        );
    }

    // --- #272: `advance_virtual_time` settles bounded reveal animations ---

    /// The core determinism property [`advance_virtual_time`]'s doc comment
    /// promises: one call jumps `elapsed`/`delta` by the *whole* requested
    /// duration at once, not a sequence of small per-frame ticks -- so a
    /// still-in-flight, time-driven animation observes a `delta_secs()` on
    /// its very next `Update` far larger than any plausible tween duration,
    /// guaranteeing it finishes rather than merely advancing one more step.
    #[test]
    fn advance_virtual_time_jumps_elapsed_by_the_whole_duration_in_one_step() {
        let mut virtual_time = Time::<Virtual>::default();
        let before = virtual_time.elapsed();

        advance_virtual_time(&mut virtual_time, 5.0);

        assert_eq!(
            virtual_time.delta(),
            std::time::Duration::from_secs_f32(5.0),
            "one call must report the whole jump as this update's delta"
        );
        assert_eq!(
            virtual_time.elapsed() - before,
            std::time::Duration::from_secs_f32(5.0),
            "elapsed must advance by exactly the requested duration, in one step"
        );
    }

    /// A second call keeps accumulating (elapsed grows monotonically) rather
    /// than resetting -- confirms this is a genuine clock advance, not a
    /// one-shot override that could mask a bug on a checkpoint that (for
    /// whatever reason) sends the command twice.
    #[test]
    fn advance_virtual_time_accumulates_across_calls() {
        let mut virtual_time = Time::<Virtual>::default();

        advance_virtual_time(&mut virtual_time, 5.0);
        advance_virtual_time(&mut virtual_time, 2.0);

        assert_eq!(
            virtual_time.elapsed(),
            std::time::Duration::from_secs_f32(7.0)
        );
    }

    /// `Duration` cannot represent a negative amount, so a malformed/negative
    /// `seconds` (never sent by the harness today, but this is the seam's own
    /// contract) must clamp to zero instead of panicking.
    #[test]
    fn advance_virtual_time_clamps_a_negative_duration_to_zero() {
        let mut virtual_time = Time::<Virtual>::default();

        advance_virtual_time(&mut virtual_time, -3.0);

        assert_eq!(virtual_time.delta(), std::time::Duration::ZERO);
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
                "ClearCorruptSave",
                ReviewButton::Menu(MenuAction::ClearCorruptSave),
            ),
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
            // #217: the paused-fight overlay's actions.
            ("PauseResume", ReviewButton::Pause(PauseAction::Resume)),
            ("PauseSettings", ReviewButton::Pause(PauseAction::Settings)),
            ("PauseAbandon", ReviewButton::Pause(PauseAction::Abandon)),
        ] {
            assert_eq!(parse_button(name), Some(expected), "{name}");
        }
    }

    /// #217: `ShopItem:<name>` addresses one of every catalog item's own
    /// buy/equip button by its stable [`ItemId`] `Debug` name -- used by the
    /// `save-reload` browser scenario to prove a shop purchase autosaves and
    /// survives a reload.
    #[test]
    fn parse_button_resolves_every_catalog_item_by_its_shop_item_command() {
        for id in ItemId::ALL {
            let name = format!("ShopItem:{id:?}");
            assert_eq!(
                parse_button(&name),
                Some(ReviewButton::Shop(ShopAction::Item(id))),
                "{name}"
            );
        }
        assert_eq!(
            parse_button("ShopItem:NuExista"),
            None,
            "an unknown item name must be rejected"
        );
    }

    #[test]
    fn parse_button_rejects_unknown_and_non_navigation_names() {
        // The automated combat-outcome routes have no button and must stay
        // unreachable from the seam; in-screen editors (attribute steppers)
        // are seeded through dedicated commands instead.
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
            .run_system_once(
                move |mut buttons: Query<
                    PressableButton,
                    (With<Button>, Without<CategoryButton>),
                >| {
                    press_button(
                        "NewGame",
                        ReviewButton::Menu(MenuAction::NewGame),
                        &mut buttons,
                    );
                },
            )
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

    /// #199: `press_category_button` finds the real `CategoryButton` entity
    /// matching the requested category and presses it -- proof the review
    /// seam drives the same production toggle a tap would, not a bypass.
    #[test]
    fn press_category_button_presses_the_matching_entity_only() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = bevy::ecs::world::World::new();
        let strikes = world
            .spawn((
                Button,
                Interaction::None,
                CategoryButton {
                    category: ActionCategory::Strikes,
                },
            ))
            .id();
        let defense = world
            .spawn((
                Button,
                Interaction::None,
                CategoryButton {
                    category: ActionCategory::Defense,
                },
            ))
            .id();

        let pressed = world
            .run_system_once(
                move |mut buttons: Query<(&mut Interaction, &CategoryButton), With<Button>>| {
                    press_category_button(ActionCategory::Strikes, &mut buttons)
                },
            )
            .expect("system runs");
        assert!(pressed);
        assert_eq!(
            *world.get::<Interaction>(strikes).unwrap(),
            Interaction::Pressed
        );
        assert_eq!(
            *world.get::<Interaction>(defense).unwrap(),
            Interaction::None
        );
    }

    #[test]
    fn press_category_button_reports_false_when_no_such_category_exists() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = bevy::ecs::world::World::new();
        world.spawn((
            Button,
            Interaction::None,
            CategoryButton {
                category: ActionCategory::Defense,
            },
        ));

        let pressed = world
            .run_system_once(
                move |mut buttons: Query<(&mut Interaction, &CategoryButton), With<Button>>| {
                    press_category_button(ActionCategory::Movement, &mut buttons)
                },
            )
            .expect("system runs");
        assert!(!pressed);
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
