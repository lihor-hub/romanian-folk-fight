//! The `keyboard-accessibility` scenario (#216, a child of #145): drives a
//! real, freshly-launched browser through every current screen -- menu,
//! settings (opened over the menu and again over the paused fight),
//! character creation, the fight HUD (action palette + pause overlay), the
//! fight result screen, and the shop -- using only real CDP-dispatched
//! `ArrowRight`/`Enter` key presses for the behavior under test, proving a
//! keyboard-only player can reach and activate every current control with a
//! visible focus marker. Extends #168's harness per the documented extension
//! pattern (see `web_smoke::mod`'s module docs): a new module here plus one
//! match arm in `web_smoke::run_scenario`.
//!
//! ## Why `ArrowRight`, not `Tab`
//!
//! Same reasoning as `fight_palette_accessible` (#213): headless Chrome's
//! `Input.dispatchKeyEvent` triggers the browser's own native Tab-focus
//! traversal on a canvas-only page, which only reliably reaches the game's
//! own keyboard handling on the first press. `ArrowRight` has no such
//! browser-level default action, so it is this harness's reliable choice.
//! `Tab`/`Shift+Tab` are covered end-to-end by every screen's own headless
//! `cargo test --lib` coverage (`ui_widgets::focus`, `menu::tests`,
//! `creation::tests`, `settings::tests`, `combat::pause::tests`,
//! `progression::result_ui::tests`, `shop::tests`), which inject
//! `KeyCode::Tab` directly and are unaffected by a browser's native key
//! handling.
//!
//! ## Reading exact focus facts instead of diffing screenshots
//!
//! `src/review/mod.rs`'s `AccessibilitySnapshot` (#216) exposes, every
//! frame: the currently focused control's own rendered label (its direct
//! `Text` child -- the same Romanian copy a player reads), whether its gold
//! focus marker is actually rendered (a non-transparent `Outline`, read from
//! the live component), and every currently-visible `Focusable` control's
//! on-screen box. This scenario presses `ArrowRight` exactly as many times
//! as there are currently-visible focusable controls (a count read from the
//! same snapshot, not hard-coded), checks the gold marker is visible after
//! every single press, and confirms one further press wraps back to the
//! first control -- proving full-screen coverage and a deterministic cyclic
//! order without needing to know every screen's exact button count or order
//! ahead of time. A handful of specific, human-checkable labels are spot-
//! checked per screen (see each `exercise_*` function) so a screen silently
//! losing a required control (not just reordering) fails loudly too.
//!
//! ## Audio settings, keyboard-only
//!
//! The settings overlay's mute toggle's own rendered label is the live
//! state ("Sunet: Pornit"/"Sunet: Oprit", see `settings::mute_label`) --
//! pressing `Enter` on it while focused and reading the label again through
//! the same snapshot is a direct, real-browser proof that audio settings
//! are operable keyboard-only (one of #216's acceptance criteria), not an
//! inference from a screenshot.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-keyboard-accessibility/` directory, mirroring every
//! other review-seam scenario, so concurrent scenario runs never clobber
//! each other's build output.
//!
//! ## No screenshot baselines
//!
//! Like `accessibility-settings-reload` and `reduced-motion-fight`, this
//! scenario's pass/fail gate is exact telemetry, not a pixel diff.
//! Screenshots are still captured as artifacts for human review.
//!
//! ## Navigate, then activate, generally (#270)
//!
//! Every "move focus to a labeled control, then press `Enter`" call site in
//! this module funnels through one pair of helpers -- [`navigate_and_activate`]
//! (settles focus via [`press_arrow_until_label`], then activates) and
//! [`activate_focused_control`] (activates whatever is already focused, for
//! the one call site that doesn't need to navigate first: the fight HUD's
//! ⏸ button right after [`walk_full_lap`] returns already sitting on it) --
//! instead of each site hand-rolling its own "press Enter, then wait a bit"
//! shape. Both funnel through the same three steps:
//!
//! 1. **Settle**: focus already sits on the intended control (verified by
//!    its exact rendered label, not an index).
//! 2. **Drain**: [`drain_pending_input`] waits for a short run of genuinely
//!    unchanging frames before `Enter` is dispatched -- see its own doc
//!    comment for the concrete CI race this closes (#270's root cause: a
//!    navigation key and the following `Enter` landing in the same slow
//!    game-side frame, so `Enter` activates whichever control focus lands
//!    on *after* the still-in-flight navigation, not the one this scenario
//!    intended).
//! 3. **Assert a specific outcome**: the caller-supplied `assert_outcome`
//!    closure polls for the *exact* fact the activation is supposed to
//!    produce -- a named screen ([`wait_for_screen`]), a specific flipped
//!    label ([`wait_for_label`]), an overlay's first control autofocusing
//!    ([`wait_for_overlay_open`]), or focus moving off the entity that was
//!    just activated ([`wait_for_focus_to_move_off`]) -- never "the
//!    snapshot changed at all" (see [`press_key_and_wait_for_change`]'s own
//!    doc comment for why that weaker check is exactly the shape of #270's
//!    four prior point-fixes, each patched individually because the next
//!    call site still had the same latent hole).
//!
//! Every one of #270's four confirmed instances (cold first `ArrowRight`,
//! the pause-overlay walk, settings-close, settings-open) is an activation
//! this shape now covers uniformly, plus the two sites (resuming from
//! pause, opening the pause overlay) that were never individually patched
//! but share the identical race surface.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "keyboard-accessibility";

const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
const REVIEW_ACCESSIBILITY_KEY: &str = "rff_review_a11y_v1";

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 800;

const KEYBOARD_ACCESSIBILITY_SEED: u64 = 20;
const KEYBOARD_ACCESSIBILITY_PRESET: &str = "Voinicul";

const READY_MAX_FRAMES: usize = 3600;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const STABLE_FRAMES_REQUIRED: usize = 3;
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;
/// Frame budget for one `ArrowRight`/`Enter` press to be reflected in the
/// next published accessibility snapshot.
const SETTLE_MAX_FRAMES: usize = 120;
/// Generous upper bound on ArrowRight presses to discover one full lap (see
/// [`walk_full_lap`]) -- comfortably above every current screen's actual
/// control count, so this is a safety bound, not a per-screen tuning knob.
const MAX_LAP_PRESSES: usize = 40;
/// Frame budget for [`drain_pending_input`] to observe
/// [`DRAIN_STABLE_FRAMES_REQUIRED`] consecutive unchanged snapshots before
/// giving up. Generous relative to the required streak -- this is a safety
/// bound against a genuinely stuck page, not a tuning knob for the common
/// case, which drains in a handful of frames.
const DRAIN_MAX_FRAMES: usize = 60;
/// Consecutive frames the published accessibility snapshot must hold
/// completely unchanged before [`drain_pending_input`] considers input
/// quiesced -- see that function's doc comment for why one unchanged frame
/// is not enough.
const DRAIN_STABLE_FRAMES_REQUIRED: usize = 3;

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (see its module docs)."
        );
    }

    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-keyboard-accessibility/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-keyboard-accessibility/ at {}",
        server.base_url()
    );

    let dir = artifacts::checkpoint_dir(SCENARIO, "journey").map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[journey]"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    let profile_dir = artifacts::scenario_dir(SCENARIO).join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let outcome = run_journey(&server, &dir, &profile_dir);
    let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));

    match outcome {
        Ok(notes) => {
            let _ = artifacts::write_artifact(&dir, "focus.log", notes.join("\n"));
            println!(
                "\n{SCENARIO}: every screen is fully keyboard-reachable with a visible focus \
                 marker -- artifacts: {}",
                dir.display()
            );
            Ok(())
        }
        Err(message) => Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}[journey]"),
            message,
            dir,
        )),
    }
}

fn build_review_release() -> Result<PathBuf, SmokeError> {
    let mut cmd = Command::new("trunk");
    cmd.arg("build")
        .arg("--release")
        .arg("--features")
        .arg("review")
        .arg("--dist")
        .arg("dist-keyboard-accessibility");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (keyboard-accessibility)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-keyboard-accessibility"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml always has a parent workspace root")
        .to_path_buf()
}

fn send_command(checkpoint: &Checkpoint, payload: serde_json::Value) -> Result<(), String> {
    let json = payload.to_string();
    let js_literal = serde_json::to_string(&json).map_err(|e| e.to_string())?;
    checkpoint.eval_unit(&format!(
        "localStorage.setItem('{REVIEW_COMMAND_KEY}', {js_literal});"
    ))?;
    for _ in 0..COMMAND_CONSUMED_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        let pending =
            checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_COMMAND_KEY}')"))?;
        if pending.is_none() {
            return Ok(());
        }
    }
    Err(format!(
        "review command was never consumed by the game within \
         {COMMAND_CONSUMED_MAX_FRAMES} frames: {json}"
    ))
}

fn read_screen(checkpoint: &Checkpoint) -> Result<Option<String>, String> {
    checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_SCREEN_KEY}')"))
}

/// Mirrors `crate::review::TargetRect`.
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
struct TargetRect {
    #[allow(dead_code)]
    x: f32,
    #[allow(dead_code)]
    y: f32,
    width: f32,
    height: f32,
}

/// Mirrors `crate::review::AccessibilitySnapshot` (#216).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct AccessibilitySnapshot {
    /// A stable per-entity identifier, used for cycle detection
    /// ([`walk_full_lap`]): several controls on one screen can share the
    /// exact same rendered `focused_label` (e.g. both volume steppers'
    /// decrease buttons render literally "-"), so `focused_label` alone
    /// cannot tell two different tab-stops apart.
    focused_entity: Option<String>,
    focused_label: Option<String>,
    focus_marker_visible: bool,
    targets: Vec<TargetRect>,
    #[allow(dead_code)]
    min_target_size: f32,
}

fn read_accessibility(checkpoint: &Checkpoint) -> Result<Option<AccessibilitySnapshot>, String> {
    let raw = checkpoint.eval_string(&format!(
        "localStorage.getItem('{REVIEW_ACCESSIBILITY_KEY}')"
    ))?;
    match raw {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("accessibility snapshot was not valid JSON ({json}): {e}")),
    }
}

/// Presses `key` via a real CDP keypress and waits until the published
/// accessibility snapshot both (a) actually differs from whatever was
/// published just before the press and (b) names a real focused control
/// (`focused_entity.is_some()`), up to [`SETTLE_MAX_FRAMES`], returning the
/// resulting snapshot. See `fight_palette_accessible::press_key_and_wait_for_change`
/// for the base rationale (comparing against a captured "before" value is
/// what makes this robust against the harness racing the browser's own
/// asynchronous dispatch/render loop).
///
/// Every call site here presses `ArrowRight` to move focus, so a resulting
/// `focused_entity: None` is never a legitimate settled outcome -- but
/// requiring *only* "differs from `before`" is not enough to guarantee that:
/// [`AccessibilitySnapshot::targets`] legitimately changes on frames that
/// have nothing to do with the just-issued key press (e.g. a focusable
/// control's on-screen box shifting as the shared widget's scroll-into-view
/// settles, or one appearing/disappearing as a screen's own UI keeps
/// finishing spawning) -- see `walk_full_lap`'s own doc comment on why a
/// wrapped-around lap "need not be byte-identical" to the snapshot it
/// started from. Read literally, the pre-#268 version of this loop would
/// return on the *first* such incidental change even if it still carried
/// `focused_entity: None`, which is exactly the shape of `keyboard-
/// accessibility`'s "the first ArrowRight press left nothing focused"
/// failure on a cold-booted MainMenu: the loop gave up one frame too early,
/// before the press's own effect (focus actually landing) had been
/// published, mistaking an unrelated field's settling for "done". Requiring
/// `focused_entity.is_some()` here keeps waiting past that kind of noise
/// instead of racing ahead of it, while still failing loudly (after
/// `SETTLE_MAX_FRAMES`) if focus genuinely never lands -- this does not
/// weaken the gate, it only stops the harness from asserting a false
/// positive against its own noisy intermediate frames.
fn press_key_and_wait_for_change(
    checkpoint: &Checkpoint,
    key: &str,
) -> Result<AccessibilitySnapshot, String> {
    let before = read_accessibility(checkpoint)?;
    checkpoint.press_key(key)?;
    for _ in 0..SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_accessibility(checkpoint)?
            && Some(&snapshot) != before.as_ref()
            && snapshot.focused_entity.is_some()
        {
            return Ok(snapshot);
        }
    }
    Err(format!(
        "the accessibility snapshot never settled on a focused control within \
         {SETTLE_MAX_FRAMES} frames after pressing {key:?} (started from {before:?})"
    ))
}

fn wait_for_screen(
    checkpoint: &Checkpoint,
    expected: &str,
    require_boot: bool,
) -> Result<(), String> {
    let (max_frames, max_wall_clock) = if require_boot {
        (READY_MAX_FRAMES, READY_MAX_WALL_CLOCK)
    } else {
        (900, Duration::from_secs(60))
    };
    let start = Instant::now();
    let mut last_screen: Option<String> = None;
    for _ in 0..max_frames {
        if start.elapsed() > max_wall_clock {
            break;
        }
        checkpoint.wait_for_frame()?;
        let screen = read_screen(checkpoint)?;
        last_screen = screen.clone();
        let status = checkpoint.read_status()?;
        if require_boot && !status.app_booted() {
            continue;
        }
        if screen.as_deref() == Some(expected) {
            return Ok(());
        }
    }
    Err(format!(
        "never observed screen `{expected}` within {max_wall_clock:?}/{max_frames} frames \
         (last seen: {last_screen:?})"
    ))
}

/// Presses `ArrowRight` repeatedly, discovering one full cyclic lap starting
/// from wherever focus currently sits -- empirically, by pressing until the
/// snapshot repeats the starting one, rather than assuming a fixed control
/// count. This is deliberately robust to a screen with an open modal
/// overlay on top: `AccessibilitySnapshot.targets` lists every currently-
/// existing `Focusable` entity in the whole world (including ones behind an
/// open modal, e.g. the main menu's own buttons while Settings sits over
/// them), so its length is *not* the same as how many stops `ArrowRight`
/// actually cycles through once focus is confined inside a modal group (see
/// `crate::ui_widgets::focus`'s registration API on `TabGroup::modal`).
/// Empirical cycle detection sidesteps that mismatch entirely. Returns the
/// starting snapshot plus every step up to (not including) the repeat, so
/// the return value already reflects one full, non-duplicated lap.
fn walk_full_lap(
    checkpoint: &Checkpoint,
    max_presses: usize,
) -> Result<Vec<AccessibilitySnapshot>, String> {
    // The first press both establishes a definite anchor (whether focus
    // started unset -- the pointer-first default -- or already sitting on
    // some entity from an earlier step) and is itself the lap's first
    // tab-stop. The cycle is detected by the stable `focused_entity` id,
    // never by whole-snapshot equality: the shared widget's
    // scroll-into-view (#216) legitimately shifts every published target
    // rect as the walk scrolls a screen, so the snapshot that wraps back
    // to the starting *control* need not be byte-identical to the starting
    // *snapshot*.
    let start = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
    let start_entity = start
        .focused_entity
        .clone()
        .ok_or_else(|| "the first ArrowRight press left nothing focused".to_string())?;
    let mut lap = vec![start];
    for _ in 0..max_presses {
        let snapshot = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
        if snapshot.focused_entity.as_deref() == Some(start_entity.as_str()) {
            return Ok(lap);
        }
        lap.push(snapshot);
    }
    Err(format!(
        "tab order never wrapped back to the starting control within {max_presses} \
         ArrowRight presses (still tabbing through: {:?})",
        lap.iter().map(|s| &s.focused_label).collect::<Vec<_>>()
    ))
}

/// Asserts every step of a full lap (as returned by [`walk_full_lap`], which
/// already stops exactly at the wrap-around point) showed the visible gold
/// focus marker.
fn assert_full_coverage(screen: &str, lap: &[AccessibilitySnapshot]) -> Result<(), String> {
    for (index, snapshot) in lap.iter().enumerate() {
        if !snapshot.focus_marker_visible {
            return Err(format!(
                "{screen}: control at tab-stop {index} ({:?}) has no visible focus marker",
                snapshot.focused_label
            ));
        }
    }
    Ok(())
}

fn assert_labels_present(
    screen: &str,
    lap: &[AccessibilitySnapshot],
    required: &[&str],
) -> Result<(), String> {
    let seen: Vec<&str> = lap
        .iter()
        .filter_map(|s| s.focused_label.as_deref())
        .collect();
    for label in required {
        if !seen.contains(label) {
            return Err(format!(
                "{screen}: required control {label:?} was never reached by keyboard; saw {seen:?}"
            ));
        }
    }
    Ok(())
}

fn run_journey(
    server: &StaticServer,
    dir: &std::path::Path,
    profile_dir: &std::path::Path,
) -> Result<Vec<String>, String> {
    let checkpoint = browser::launch(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, 1.0, profile_dir)?;
    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url)?;

    let mut notes = Vec::new();

    wait_for_screen(&checkpoint, "MainMenu", true)?;
    exercise_main_menu(&checkpoint, &mut notes)?;

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "seedCombat", "seed": KEYBOARD_ACCESSIBILITY_SEED}),
    )?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
    )?;
    wait_for_screen(&checkpoint, "CharacterCreation", false)?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "selectPreset", "preset": KEYBOARD_ACCESSIBILITY_PRESET}),
    )?;
    exercise_creation(&checkpoint, &mut notes)?;

    // Confirm the hero via the real focused Confirm button, keyboard-only --
    // the whole point of this scenario, rather than the `pressButton` shim
    // every other journey scenario uses for navigation.
    navigate_and_activate(&checkpoint, "Începe lupta", |checkpoint, _before| {
        wait_for_screen(checkpoint, "Fight", false)
    })?;
    // The fight screen animates continuously (parallax drift, idle sprite
    // frames) -- freeze `Time<Virtual>` so the byte-identical-frames
    // stability streak can land, the exact same pause/capture pattern
    // `gold_journey::captured_checkpoint` documents. Focus navigation is
    // unaffected: the focus systems read `ButtonInput` in `Update`,
    // independent of the virtual clock.
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )?;
    wait_for_stable_frames(&checkpoint)?;

    exercise_fight(&checkpoint, &mut notes)?;

    // Unpause: autoplay (and the fight-end delay after it) need the clock.
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    )?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
    )?;
    wait_for_screen(&checkpoint, "FightResult", false)?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )?;
    wait_for_stable_frames(&checkpoint)?;
    exercise_result(&checkpoint, &mut notes)?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    )?;

    navigate_and_activate(&checkpoint, "La prăvălie", |checkpoint, _before| {
        wait_for_screen(checkpoint, "Shop", false)
    })?;
    // The shop's cutout preview rig idles too -- same freeze.
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )?;
    wait_for_stable_frames(&checkpoint)?;
    exercise_shop(&checkpoint, &mut notes)?;
    let _ = send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    );

    let final_shot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
    let _ = artifacts::write_artifact(dir, "final-shop.png", &final_shot);

    let status = checkpoint.read_status()?;
    check_no_console_or_page_errors(&status)?;
    let _ = artifacts::write_artifact(dir, "console.log", status.console.join("\n"));

    Ok(notes)
}

fn check_no_console_or_page_errors(status: &PageStatus) -> Result<(), String> {
    if !status.errors.is_empty() {
        return Err(format!("page-level errors observed: {:?}", status.errors));
    }
    let console_errors: Vec<&String> = status
        .console
        .iter()
        .filter(|line| line.starts_with("error:"))
        .collect();
    if !console_errors.is_empty() {
        return Err(format!("console.error observed: {console_errors:?}"));
    }
    Ok(())
}

fn wait_for_stable_frames(checkpoint: &Checkpoint) -> Result<(), String> {
    let mut last: Option<Vec<u8>> = None;
    let mut stable = 0usize;
    for _ in 0..600 {
        checkpoint.wait_for_frame()?;
        let shot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
        if last.as_deref() == Some(shot.as_slice()) {
            stable += 1;
        } else {
            stable = 1;
        }
        last = Some(shot);
        if stable >= STABLE_FRAMES_REQUIRED {
            return Ok(());
        }
    }
    Err("screen never stabilized within 600 frames".to_string())
}

/// Presses `ArrowRight` (up to a generous bound) until the focused control's
/// label matches `label` exactly, so callers never have to hard-code a
/// screen's exact tab-stop index.
fn press_arrow_until_label(checkpoint: &Checkpoint, label: &str) -> Result<(), String> {
    for _ in 0..64 {
        let snapshot = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
        if snapshot.focused_label.as_deref() == Some(label) {
            return Ok(());
        }
    }
    Err(format!(
        "never reached a control labeled {label:?} within 64 ArrowRight presses"
    ))
}

/// The pure decision behind [`drain_pending_input`]'s consecutive-frame
/// counter: given the previously observed snapshot, the one just read, and
/// how many consecutive identical frames preceded it, returns the new streak
/// length -- `1` whenever the snapshot moved at all (a fresh streak starts at
/// the frame that first held still), the old streak plus one otherwise. This
/// is the one piece of actual *decision* logic inside an otherwise pure
/// browser-polling loop, extracted as a free function so it is directly
/// unit-testable without a real browser (see the `tests` module below).
fn advance_stable_streak(
    previous: &Option<AccessibilitySnapshot>,
    current: &Option<AccessibilitySnapshot>,
    streak: usize,
) -> usize {
    if current == previous { streak + 1 } else { 1 }
}

/// Waits for [`DRAIN_STABLE_FRAMES_REQUIRED`] consecutive frames in which the
/// published accessibility snapshot does not change at all -- the "drain"
/// half of the navigate-then-activate hardening this module's docs describe
/// (#270). On slow CI, the browser's asynchronous CDP key-dispatch pipeline
/// can still be delivering the effects of a just-settled navigation key for
/// a frame or two after this scenario's own settle loop
/// ([`press_key_and_wait_for_change`]) already observed the snapshot move
/// once -- that loop only proves the press *started* landing, not that
/// nothing from it is still in flight. Pressing `Enter` immediately after
/// only the first observed change risks exactly the race #270 describes:
/// `Enter` lands in the same slow game-side frame as a still-in-flight
/// navigation keypress, the focus-navigation system (which runs before
/// button-activation handlers in `Update`) moves focus again before the
/// activation is even read, and `Enter` fires on whatever control focus
/// just landed on instead of the one this scenario intended. Requiring a
/// short run of genuinely unchanging frames -- not just "the frame right
/// after the one that changed" -- is what actually proves no navigation
/// input is still working its way through. Bounded by [`DRAIN_MAX_FRAMES`];
/// never observing that stable run is a diagnosed failure, not a silent
/// fall-through into pressing `Enter` anyway.
fn drain_pending_input(checkpoint: &Checkpoint) -> Result<(), String> {
    let mut last = read_accessibility(checkpoint)?;
    let mut streak = 0usize;
    for _ in 0..DRAIN_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        let current = read_accessibility(checkpoint)?;
        streak = advance_stable_streak(&last, &current, streak);
        last = current;
        if streak >= DRAIN_STABLE_FRAMES_REQUIRED {
            return Ok(());
        }
    }
    Err(format!(
        "input never quiesced -- the accessibility snapshot kept changing for {DRAIN_MAX_FRAMES} \
         frames without holding still for {DRAIN_STABLE_FRAMES_REQUIRED} in a row (last observed: \
         {last:?})"
    ))
}

/// The one hardened "activate the currently focused control, then reach a
/// *specific* new state" primitive every activation site in this scenario
/// funnels through (#270): drains pending input ([`drain_pending_input`]),
/// captures the snapshot immediately before pressing `Enter` (passed to
/// `assert_outcome` as `before`, so callers that need it -- e.g. "focus must
/// move off the entity that was just focused" -- never race a second,
/// separately-timed read against this one), presses `Enter`, then hands
/// control to `assert_outcome`.
///
/// `assert_outcome` must itself poll for a *specific* expected fact (a named
/// screen, an exact focused label, focus having moved off a named entity,
/// ...), never "the snapshot changed at all" -- seeded a bounded loop of the
/// caller's choosing, since what counts as "done" differs per call site
/// (some open something and must confirm a new autofocus target, others
/// close something and must confirm focus left; see [`wait_for_screen`],
/// [`wait_for_label`], [`wait_for_overlay_open`], and
/// [`wait_for_focus_to_move_off`] for this module's four such predicates).
///
/// ## One bounded retry for a provably-lost press (#305's CI failures)
///
/// PR #305's CI hit this scenario's settle assertions twice, at two
/// different Enter activations (the settings overlay never autofocusing
/// `"-"`; the mute label never flipping) -- both after a settled, drained
/// Enter, both with 120 further rendered frames showing *no effect at all*.
/// A press that reached the game resolves in a tick or two, so the only
/// consistent reading is a CDP key press lost (or diverted) under CI load.
/// When `assert_outcome` times out, this function therefore re-reads the
/// snapshot and -- only if [`enter_press_left_no_trace`] proves the press
/// had no observable effect (focus still on the same entity with the same
/// rendered label) -- presses `Enter` once more and re-asserts. A press
/// that *did* do something (label flipped late, focus moved) never
/// re-fires, so no activation can double-toggle; and a genuinely broken
/// autofocus still fails loudly, now with both attempts' errors.
fn activate_focused_control(
    checkpoint: &Checkpoint,
    assert_outcome: impl Fn(&Checkpoint, Option<AccessibilitySnapshot>) -> Result<(), String>,
) -> Result<(), String> {
    drain_pending_input(checkpoint)?;
    let before = read_accessibility(checkpoint)?;
    checkpoint.press_key("Enter")?;
    let first_error = match assert_outcome(checkpoint, before.clone()) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let current = read_accessibility(checkpoint)?;
    if !enter_press_left_no_trace(&before, &current) {
        return Err(first_error);
    }
    checkpoint.press_key("Enter")?;
    assert_outcome(checkpoint, before).map_err(|retry_error| {
        format!(
            "{retry_error} (after one retry for an apparently lost Enter press; \
             first attempt: {first_error})"
        )
    })
}

/// Pure decision behind [`activate_focused_control`]'s single retry: `true`
/// only when the post-timeout snapshot proves the `Enter` press had no
/// observable effect whatsoever -- focus still sits on the same entity
/// (`focused_entity`) *and* that control still renders the same label
/// (`focused_label`, which an in-place toggle like the mute button changes
/// even though the entity stays put). Anything else -- a moved focus, a
/// flipped label, or either snapshot missing (nothing provable) -- returns
/// `false`, so the retry can never re-activate a control whose first press
/// actually landed.
fn enter_press_left_no_trace(
    before: &Option<AccessibilitySnapshot>,
    current: &Option<AccessibilitySnapshot>,
) -> bool {
    match (before, current) {
        (Some(before), Some(current)) => {
            before.focused_entity == current.focused_entity
                && before.focused_label == current.focused_label
        }
        _ => false,
    }
}

/// [`activate_focused_control`], but first settles focus on `label` via
/// [`press_arrow_until_label`] -- the "navigate" half of the navigate-then-
/// activate model. Every call site that needs to move focus to a named
/// control before activating it goes through this; the one exception is the
/// fight HUD's ⏸ button, which [`walk_full_lap`] already leaves focus
/// sitting on when it returns, so that call site uses
/// [`activate_focused_control`] directly instead of navigating to a label it
/// is already on.
fn navigate_and_activate(
    checkpoint: &Checkpoint,
    label: &str,
    assert_outcome: impl Fn(&Checkpoint, Option<AccessibilitySnapshot>) -> Result<(), String>,
) -> Result<(), String> {
    press_arrow_until_label(checkpoint, label)?;
    activate_focused_control(checkpoint, assert_outcome)
}

/// [`activate_focused_control`]/[`navigate_and_activate`] outcome: waits for
/// `InputFocus` to actually move off whatever entity `before` named -- the
/// precise, race-free signal that an activation which despawns or closes
/// something (the settings overlay's **Înapoi**, the pause overlay's
/// **Continuă lupta**) has actually been processed by the game, replacing a
/// fixed-frame settle or a `read_screen(..) == parent_screen` poll that can
/// be trivially true *before* the activation has even fired (opening/closing
/// either overlay is a `Resource` change, never a `GameState` change, so
/// that condition holds on the very first polled frame regardless of
/// whether anything happened yet).
///
/// This is deliberately not "wait for the whole snapshot to differ from
/// `before`": over the paused fight, closing the settings overlay refocuses
/// the pause panel's own first control; over the main menu, it clears focus
/// to `None` (`core::despawn_screen`'s doc comment); resuming from pause
/// always clears focus to `None` the same way. All three leave
/// `focused_entity` different from what it was -- the one fact every one of
/// these activations actually guarantees -- while an incidental
/// `targets`-rect shift (the shared widget's scroll-into-view settling, see
/// `walk_full_lap`'s doc comment) can make the *whole* snapshot differ while
/// focus is still sitting on the very entity that was just activated, which
/// is the same false-positive class #268 first found and fixed here (this
/// function generalizes that fix, previously named
/// `press_enter_and_wait_for_overlay_close` and settings-close-only, to
/// every activation that is expected to move focus off itself).
fn wait_for_focus_to_move_off(
    checkpoint: &Checkpoint,
    before: Option<AccessibilitySnapshot>,
) -> Result<(), String> {
    let before_focus = before.and_then(|s| s.focused_entity);
    for _ in 0..SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_accessibility(checkpoint)?
            && snapshot.focused_entity != before_focus
        {
            return Ok(());
        }
    }
    Err(format!(
        "activating the focused control never moved focus off it within {SETTLE_MAX_FRAMES} \
         frames (focus was {before_focus:?})"
    ))
}

/// [`navigate_and_activate`] outcome: waits for the focused control's
/// rendered label to become exactly `expected` -- e.g. confirming the mute
/// toggle's label actually flipped to the specific expected polarity
/// ([`flipped_mute_label`]), rather than accepting the first incidental
/// snapshot change (a `targets`-rect shift has nothing to do with the label
/// this scenario actually cares about, the same reasoning
/// [`wait_for_focus_to_move_off`]'s doc comment gives).
fn wait_for_label(checkpoint: &Checkpoint, expected: &str) -> Result<(), String> {
    for _ in 0..SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_accessibility(checkpoint)?
            && snapshot.focused_label.as_deref() == Some(expected)
        {
            return Ok(());
        }
    }
    Err(format!(
        "the focused control's label never became {expected:?} within {SETTLE_MAX_FRAMES} frames"
    ))
}

/// [`navigate_and_activate`]/[`activate_focused_control`] outcome for opening
/// a modal overlay (settings or pause): waits for the overlay's own first
/// control to actually autofocus, identified by its exact rendered label
/// (the settings panel's first stepper's `-` button, per
/// `settings::spawn_overlay`'s spawn order, or the pause panel's **Continuă
/// lupta**, per `combat::pause::spawn_overlay`'s), then confirms the
/// underlying `GameState` did not change (opening either overlay is a
/// `Resource` insert, never a state transition).
///
/// Waiting for the specific first-control label -- not a looser "some label
/// changed" or a fixed settle -- is what actually proves the overlay's own
/// autofocus system has run, replacing this module's previous settings-open
/// check (`l.starts_with("Muzică") || l == "-"`, which accepted a label
/// ("Muzică") no focused control could ever actually render, since it names
/// the stepper row's static, non-focusable text rather than either of its
/// buttons' own direct-child labels) and its previous pause-open check
/// (a loop that silently fell through without erroring if **Continuă
/// lupta** never autofocused within budget).
fn wait_for_overlay_open(
    checkpoint: &Checkpoint,
    first_control_label: &str,
    parent_screen: &str,
) -> Result<(), String> {
    for _ in 0..SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_accessibility(checkpoint)?
            && snapshot.focused_label.as_deref() == Some(first_control_label)
        {
            let screen = read_screen(checkpoint)?;
            return if screen.as_deref() == Some(parent_screen) {
                Ok(())
            } else {
                Err(format!(
                    "opening the overlay must not change the GameState (still {parent_screen}), \
                     observed {screen:?}"
                ))
            };
        }
    }
    Err(format!(
        "the overlay never autofocused its first control ({first_control_label:?}) within \
         {SETTLE_MAX_FRAMES} frames"
    ))
}

/// The mute toggle's label after flipping, given its current rendered label
/// -- computed purely from the label text (never queried from the game
/// itself) so [`navigate_and_activate`]'s mute-toggle outcome can require the
/// *specific* flipped value via [`wait_for_label`], not just "the label
/// changed to something". Mirrors `settings::mute_label`'s two literal
/// strings (duplicated here, not shared code -- the same reasoning
/// `REVIEW_COMMAND_KEY` above documents: this dev-tooling crate never
/// depends on the game crate).
fn flipped_mute_label(current: &str) -> &'static str {
    if current == "Sunet: Pornit" {
        "Sunet: Oprit"
    } else {
        "Sunet: Pornit"
    }
}

/// The main menu: **Luptă nouă**, the disabled **Continuă** marker, and
/// **Setări** must all be reachable with the visible marker (**Ieși** is
/// native-only, `#[cfg(not(target_arch = "wasm32"))]` -- this scenario only
/// ever runs against the wasm build this harness serves, so it never
/// spawns); opening **Setări** and exercising its controls (including a real
/// keyboard-only mute toggle) must not change the screen; **Înapoi** must
/// close it again.
fn exercise_main_menu(checkpoint: &Checkpoint, notes: &mut Vec<String>) -> Result<(), String> {
    let lap = walk_full_lap(checkpoint, MAX_LAP_PRESSES)?;
    assert_full_coverage("MainMenu", &lap)?;
    assert_labels_present("MainMenu", &lap, &["Luptă nouă", "Continuă", "Setări"])?;
    notes.push(format!(
        "MainMenu: {} controls, all keyboard-reachable with a visible marker",
        lap.len()
    ));

    exercise_settings_overlay(checkpoint, "MainMenu", notes)?;
    Ok(())
}

/// Opens the settings overlay (focused on **Setări** already, from the
/// caller's own full-lap walk), exercises every control keyboard-only
/// (including toggling mute and reading its label change back, proving
/// audio settings are operable keyboard-only), then closes it via
/// **Înapoi** and confirms the screen underneath is unchanged.
fn exercise_settings_overlay(
    checkpoint: &Checkpoint,
    parent_screen: &str,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    // Opening the overlay is not a `GameState` change; the overlay's own
    // autofocus must land on its first control (the Muzică stepper's `-`
    // button, see `settings::spawn_overlay`'s spawn order) before this
    // function is willing to walk its controls -- see `navigate_and_activate`
    // and `wait_for_overlay_open`'s doc comments for why this replaced a
    // looser "some label starting with Muzică or -" check plus a separate,
    // unretried `GameState` read.
    navigate_and_activate(checkpoint, "Setări", |checkpoint, _before| {
        wait_for_overlay_open(checkpoint, "-", parent_screen)
    })?;

    let lap = walk_full_lap(checkpoint, MAX_LAP_PRESSES)?;
    assert_full_coverage("Settings", &lap)?;

    // Keyboard-only audio operability: find the mute toggle by its live
    // label (either polarity), toggle it, and confirm the rendered label
    // actually flipped to the specific expected polarity (not just "some
    // change was observed" -- see `flipped_mute_label`'s doc comment).
    let mute_before = lap
        .iter()
        .filter_map(|s| s.focused_label.as_deref())
        .find(|label| label.starts_with("Sunet:"))
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "Settings: the mute toggle («Sunet: ...») was never reached; lap labels: {:?}",
                lap.iter().map(|s| &s.focused_label).collect::<Vec<_>>()
            )
        })?;
    let mute_after = flipped_mute_label(&mute_before);
    navigate_and_activate(checkpoint, &mute_before, |checkpoint, _before| {
        wait_for_label(checkpoint, mute_after)
    })?;
    notes.push(format!(
        "Settings ({parent_screen}): {} controls, mute toggled {mute_before:?} -> \
         {mute_after:?} keyboard-only",
        lap.len()
    ));

    navigate_and_activate(checkpoint, "Înapoi", |checkpoint, before| {
        wait_for_focus_to_move_off(checkpoint, before)
    })?;
    Ok(())
}

/// Character creation: every preset tile, the name arrows, the appearance
/// and attribute steppers, and Confirm/Back must all be reachable.
fn exercise_creation(checkpoint: &Checkpoint, notes: &mut Vec<String>) -> Result<(), String> {
    let lap = walk_full_lap(checkpoint, MAX_LAP_PRESSES)?;
    assert_full_coverage("CharacterCreation", &lap)?;
    assert_labels_present(
        "CharacterCreation",
        &lap,
        &["Personalizat", "Începe lupta", "Înapoi"],
    )?;
    notes.push(format!(
        "CharacterCreation: {} controls, all keyboard-reachable with a visible marker",
        lap.len()
    ));
    Ok(())
}

/// The fight HUD: the ⏸ button (first, ahead of the palette) and every
/// desktop action button must be reachable; opening the pause overlay via a
/// real Enter press must not change the screen, its own controls must be
/// fully reachable, and Resume must return focus to the fight screen.
fn exercise_fight(checkpoint: &Checkpoint, notes: &mut Vec<String>) -> Result<(), String> {
    let lap = walk_full_lap(checkpoint, MAX_LAP_PRESSES)?;
    assert_full_coverage("Fight", &lap)?;
    // The HUD's own ⏸ button ("||", see `combat::hud::pause_button`'s
    // doc comment on why its label isn't the "⏸" glyph) must be the very
    // first stop, ahead of the palette, per its dedicated `TabGroup::new(-1)`.
    if lap[0].focused_label.as_deref() != Some("||") {
        return Err(format!(
            "Fight: expected the ⏸ button (label \"||\") to be the first tab-stop, got {:?}",
            lap[0].focused_label
        ));
    }
    notes.push(format!(
        "Fight: {} controls (first: ⏸ button), all keyboard-reachable",
        lap.len()
    ));

    // `walk_full_lap` leaves live focus back on `lap[0]` (the ⏸ button) once
    // it returns -- open the pause overlay for real from there. No
    // navigation needed (focus is already on the ⏸ button), so this uses
    // `activate_focused_control` directly rather than `navigate_and_activate`.
    activate_focused_control(checkpoint, |checkpoint, _before| {
        wait_for_overlay_open(checkpoint, "Continuă lupta", "Fight")
    })?;

    let pause_lap = walk_full_lap(checkpoint, MAX_LAP_PRESSES)?;
    assert_full_coverage("Pause", &pause_lap)?;
    assert_labels_present(
        "Pause",
        &pause_lap,
        &["Continuă lupta", "Setări", "Abandonează"],
    )?;
    notes.push(format!(
        "Pause: {} controls, all keyboard-reachable with a visible marker",
        pause_lap.len()
    ));

    exercise_settings_overlay(checkpoint, "Fight", notes)?;

    // Resuming despawns the pause overlay and clears focus
    // (`core::despawn_screen`'s doc comment) -- the same "focus must move
    // off the entity that was just activated" shape settings-close uses, so
    // this reuses `wait_for_focus_to_move_off` instead of a fixed settle
    // (which cannot itself fail loudly if the resume never actually landed).
    navigate_and_activate(checkpoint, "Continuă lupta", |checkpoint, before| {
        wait_for_focus_to_move_off(checkpoint, before)
    })?;
    let resumed_screen = read_screen(checkpoint)?;
    if resumed_screen.as_deref() != Some("Fight") {
        return Err(format!(
            "resuming must return to the Fight screen, observed {resumed_screen:?}"
        ));
    }
    Ok(())
}

/// The fight result screen: **La prăvălie** and **Lupta următoare** (plus
/// any level-up allocation row) must be reachable.
fn exercise_result(checkpoint: &Checkpoint, notes: &mut Vec<String>) -> Result<(), String> {
    let lap = walk_full_lap(checkpoint, MAX_LAP_PRESSES)?;
    assert_full_coverage("FightResult", &lap)?;
    assert_labels_present("FightResult", &lap, &["La prăvălie", "Lupta următoare"])?;
    notes.push(format!(
        "FightResult: {} controls, all keyboard-reachable with a visible marker",
        lap.len()
    ));
    Ok(())
}

/// The shop: every catalog buy/equip button and **Înapoi în arenă** must be
/// reachable, and Enter on a focused buy button must debit/equip like a
/// click (proven headlessly already by
/// `shop::tests::enter_on_a_focused_buy_button_debits_and_equips_like_a_click`;
/// this is the real-browser plausibility check that the same key press
/// actually reaches the shop's own handler).
fn exercise_shop(checkpoint: &Checkpoint, notes: &mut Vec<String>) -> Result<(), String> {
    let lap = walk_full_lap(checkpoint, MAX_LAP_PRESSES)?;
    assert_full_coverage("Shop", &lap)?;
    assert_labels_present("Shop", &lap, &["Înapoi în arenă"])?;
    notes.push(format!(
        "Shop: {} controls, all keyboard-reachable with a visible marker",
        lap.len()
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(entity: &str, label: &str) -> AccessibilitySnapshot {
        AccessibilitySnapshot {
            focused_entity: Some(entity.to_string()),
            focused_label: Some(label.to_string()),
            focus_marker_visible: true,
            targets: Vec::new(),
            min_target_size: 0.0,
        }
    }

    // `enter_press_left_no_trace` -- the pure decision behind
    // `activate_focused_control`'s single lost-press retry (#305's CI
    // failures). These pin the exact "no observable effect" contract that
    // makes the retry safe: it may only fire when nothing at all happened.

    #[test]
    fn a_press_with_identical_focus_and_label_left_no_trace() {
        let before = Some(snapshot("7v0", "Setări"));
        let current = Some(snapshot("7v0", "Setări"));
        assert!(enter_press_left_no_trace(&before, &current));
    }

    #[test]
    fn a_flipped_label_on_the_same_entity_is_a_landed_press_never_retried() {
        // The mute toggle flips its own label in place: same entity,
        // different label. Retrying here would double-toggle.
        let before = Some(snapshot("7v0", "Sunet: Pornit"));
        let current = Some(snapshot("7v0", "Sunet: Oprit"));
        assert!(!enter_press_left_no_trace(&before, &current));
    }

    #[test]
    fn moved_focus_is_a_landed_press_never_retried() {
        // Opening the settings overlay autofocuses its first control: a
        // late-landing open must poll onward, not press Enter again (which
        // would hit the freshly focused volume stepper).
        let before = Some(snapshot("7v0", "Setări"));
        let current = Some(snapshot("12v0", "-"));
        assert!(!enter_press_left_no_trace(&before, &current));
    }

    #[test]
    fn cleared_focus_is_a_landed_press_never_retried() {
        // Closing an overlay over the main menu clears focus to None
        // (`core::despawn_screen`) -- observable effect, no retry.
        let before = Some(snapshot("7v0", "Înapoi"));
        let current = Some(AccessibilitySnapshot {
            focused_entity: None,
            focused_label: None,
            focus_marker_visible: false,
            targets: Vec::new(),
            min_target_size: 0.0,
        });
        assert!(!enter_press_left_no_trace(&before, &current));
    }

    #[test]
    fn missing_snapshots_prove_nothing_and_never_retry() {
        let some = Some(snapshot("7v0", "Setări"));
        assert!(!enter_press_left_no_trace(&None, &some));
        assert!(!enter_press_left_no_trace(&some, &None));
        assert!(!enter_press_left_no_trace(&None, &None));
    }

    // `advance_stable_streak` -- the pure decision behind `drain_pending_input`
    // (#270's "drain" half). Red-first: these pin the exact streak-counting
    // contract `drain_pending_input`'s loop relies on to ever return `Ok`.

    #[test]
    fn advance_stable_streak_starts_a_fresh_streak_of_one_on_the_first_frame() {
        let before_anything = None;
        let first_read = Some(snapshot("1v0", "Setări"));
        assert_eq!(advance_stable_streak(&before_anything, &first_read, 0), 1);
    }

    #[test]
    fn advance_stable_streak_increments_while_unchanged() {
        let a = Some(snapshot("1v0", "Setări"));
        assert_eq!(advance_stable_streak(&a, &a.clone(), 1), 2);
        assert_eq!(advance_stable_streak(&a, &a.clone(), 2), 3);
    }

    #[test]
    fn advance_stable_streak_resets_to_one_on_any_change() {
        let a = Some(snapshot("1v0", "Setări"));
        let b = Some(snapshot("2v0", "Sunet: Pornit"));
        assert_eq!(advance_stable_streak(&a, &b, 5), 1);
    }

    #[test]
    fn advance_stable_streak_resets_even_on_a_label_only_change() {
        // A `targets`-rect shift is not the only thing that can move while
        // `focused_entity` stays put -- this pins that *any* field differing
        // (not just the entity id) still resets the streak, since the whole
        // snapshot is compared.
        let a = Some(snapshot("1v0", "Sunet: Pornit"));
        let b = Some(snapshot("1v0", "Sunet: Oprit"));
        assert_eq!(advance_stable_streak(&a, &b, 2), 1);
    }

    #[test]
    fn advance_stable_streak_treats_two_consecutive_nones_as_unchanged() {
        assert_eq!(advance_stable_streak(&None, &None, 1), 2);
    }

    // `flipped_mute_label` -- the pure computation behind the mute-toggle
    // outcome assertion (`wait_for_label`'s expected value). Red-first: this
    // is the one piece of "what specific value do we expect" logic that used
    // to be implicit in "the label differs from what it was" -- #270 asks
    // for the specific expected outcome instead.

    #[test]
    fn flipped_mute_label_swaps_pornit_to_oprit() {
        assert_eq!(flipped_mute_label("Sunet: Pornit"), "Sunet: Oprit");
    }

    #[test]
    fn flipped_mute_label_swaps_oprit_to_pornit() {
        assert_eq!(flipped_mute_label("Sunet: Oprit"), "Sunet: Pornit");
    }
}
