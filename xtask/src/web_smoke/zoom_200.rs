//! The `zoom-200` scenario (#216, a child of #145): drives a real,
//! freshly-launched browser through every current screen at a viewport
//! modeling a desktop browser window zoomed to 200%, and asserts no
//! unexpected page scroll and no required control clipped outside the
//! viewport. Extends #168's harness per the documented extension pattern
//! (see `web_smoke::mod`'s module docs): a new module here plus one match
//! arm in `web_smoke::run_scenario`.
//!
//! ## Modeling "200% browser zoom" over CDP
//!
//! Chrome's own page-zoom (Ctrl/Cmd-+) has no dedicated CDP knob outside
//! mobile-emulation "page scale factor" (which this harness's headless,
//! desktop-mode launch does not use, per `browser::launch`'s doc comment).
//! What zooming a *desktop* browser window to 200% actually does to the
//! page, though, is well-defined: the same physical window now reports half
//! as many CSS pixels (`window.innerWidth`/`innerHeight` halve; content that
//! used to fit in 1280 CSS px of width now needs 2560 to render at the same
//! visual size). [`ZOOMED_WIDTH`]/[`ZOOMED_HEIGHT`] -- exactly half
//! `gold_journey`'s desktop baseline (1280x800 -> 640x400) -- reproduce that
//! effect directly via `Emulation.setDeviceMetricsOverride`'s CSS
//! width/height at device pixel ratio 1 (no Retina-style scaling folded in),
//! the same mechanism `browser::launch` already uses for every other
//! viewport in this harness. This is a real, verifiable stand-in for "200%
//! desktop zoom" as far as any web page (canvas-rendered or not) can tell.
//!
//! Crossing `theme::MOBILE_BREAKPOINT` (700px) at 640px width means the game
//! reflows to its mobile-responsive layout at this zoom level -- exactly the
//! outcome a real 200%-zoomed desktop session should produce, not a bug in
//! this scenario's viewport choice.
//!
//! ## Reading exact clipping facts instead of guessing from a screenshot
//!
//! `src/review/mod.rs`'s `AccessibilitySnapshot` (#216) publishes the
//! focused control's on-screen box in logical (CSS) pixels -- the same
//! coordinate space `window.innerWidth`/`innerHeight` are expressed in,
//! since the UI camera spans the whole window (never letterboxed, unlike
//! the fixed 4:3 arena-world camera; see `core::UiCamera`'s doc comment).
//!
//! "No clipped required control" is asserted *per focused control*, not as
//! "every control fits one viewport simultaneously": several screens are
//! designed to scroll on short viewports (`ui_widgets::Scrollable`, #31),
//! and the shared focus widget scrolls the focused control into view
//! (#216, `ui_widgets::focus::scroll_focused_into_view`). So the gate
//! walks focus through one full lap of each screen's controls and requires
//! each stop's post-scroll box to lie inside the viewport -- exactly the
//! "playable at 200% zoom" a keyboard-only player experiences. A control
//! that stays off-screen even after scroll-into-view fails the run.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-zoom-200/` directory, mirroring every other
//! review-seam scenario.
//!
//! ## No screenshot baselines
//!
//! Like `accessibility-settings-reload`, this scenario's pass/fail gate is
//! exact telemetry, not a pixel diff. Screenshots are still captured as
//! artifacts for human review.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "zoom-200";

const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
const REVIEW_ACCESSIBILITY_KEY: &str = "rff_review_a11y_v1";

/// Half of `gold_journey`'s 1280x800 desktop baseline -- see the module docs
/// for why this reproduces "a desktop window zoomed to 200%".
const ZOOMED_WIDTH: u32 = 640;
const ZOOMED_HEIGHT: u32 = 400;

const KEYBOARD_ACCESSIBILITY_SEED: u64 = 22;
const KEYBOARD_ACCESSIBILITY_PRESET: &str = "Voinicul";

const READY_MAX_FRAMES: usize = 3600;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const STABLE_FRAMES_REQUIRED: usize = 3;
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;
const SETTLE_MAX_FRAMES: usize = 120;
/// Generous upper bound on ArrowRight presses to discover one full lap --
/// comfortably above every current screen's actual control count.
const MAX_LAP_PRESSES: usize = 40;

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (see its module docs)."
        );
    }

    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-zoom-200/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-zoom-200/ at {}",
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

    let outcome = run_checks(&server, &dir, &profile_dir);
    let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));

    match outcome {
        Ok(()) => {
            println!(
                "\n{SCENARIO}: every screen stayed unscrolled and unclipped at \
                 {ZOOMED_WIDTH}x{ZOOMED_HEIGHT} (200% desktop zoom) -- artifacts: {}",
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
        .arg("dist-zoom-200");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (zoom-200)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-zoom-200"))
}

fn workspace_root() -> PathBuf {
    crate::process::workspace_root()
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
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// Mirrors `crate::review::AccessibilitySnapshot` (#216).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct AccessibilitySnapshot {
    /// Stable per-entity identifier for cycle detection (see
    /// `keyboard_accessibility`'s mirror of the same field).
    focused_entity: Option<String>,
    focused_label: Option<String>,
    focus_marker_visible: bool,
    /// The focused control's current on-screen box, *after* the shared
    /// widget's scroll-into-view ran -- the box this scenario's clipping
    /// gate checks per tab-stop.
    focused_rect: Option<TargetRect>,
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
        {
            return Ok(snapshot);
        }
    }
    Err(format!(
        "the accessibility snapshot never changed within {SETTLE_MAX_FRAMES} frames after \
         pressing {key:?} (still {before:?})"
    ))
}

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

fn wait_for_stable_frames(checkpoint: &Checkpoint) -> Result<(), String> {
    let mut last: Option<Vec<u8>> = None;
    let mut stable = 0usize;
    for _ in 0..600 {
        checkpoint.wait_for_frame()?;
        let shot = checkpoint.screenshot_png(ZOOMED_WIDTH, ZOOMED_HEIGHT)?;
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

/// No unexpected document scroll: `scrollWidth`/`scrollHeight` must not
/// exceed `clientWidth`/`clientHeight` (the same check `cold_menu` and
/// `fight_palette_accessible` apply at their own viewports).
fn check_no_unexpected_scroll(status: &PageStatus) -> Result<(), String> {
    const EPSILON: f64 = 1.0;
    if status.scroll_width > status.client_width + EPSILON {
        problem_scroll("horizontally", status.scroll_width, status.client_width)?;
    }
    if status.scroll_height > status.client_height + EPSILON {
        problem_scroll("vertically", status.scroll_height, status.client_height)?;
    }
    Ok(())
}

fn problem_scroll(axis: &str, scroll: f64, client: f64) -> Result<(), String> {
    Err(format!(
        "document scrolls {axis} at 200% zoom: scroll={scroll} > client={client}"
    ))
}

/// Whether `rect` lies fully inside the `width`x`height` viewport.
fn rect_in_viewport(rect: &TargetRect, width: f32, height: f32) -> bool {
    rect.x >= -EPSILON_PX
        && rect.y >= -EPSILON_PX
        && rect.x + rect.width <= width + EPSILON_PX
        && rect.y + rect.height <= height + EPSILON_PX
}

/// Sub-pixel slack for the visibility check: layout rounding at scaled
/// viewports can leave a box a fraction of a px past the edge.
const EPSILON_PX: f32 = 1.0;

/// The clipping gate (see the module docs): walks focus through one full
/// cyclic lap of the screen's controls (`ArrowRight` until the focused
/// entity repeats, same empirical cycle detection as
/// `keyboard_accessibility::walk_full_lap`) and asserts every stop's
/// *post-scroll* `focused_rect` lies inside the viewport. Screens designed
/// to scroll on short viewports (`ui_widgets::Scrollable`, #31) pass by
/// virtue of the shared widget's scroll-into-view-on-focus (#216); a control
/// that stays off-screen even after that is a genuine clipping failure.
/// Because `UiGlobalTransform` only reflects a scroll adjustment on the
/// *next* frame's layout pass, each stop polls a few extra frames for the
/// rect to come into view before declaring it clipped.
fn assert_every_control_visible_when_focused(
    checkpoint: &Checkpoint,
    screen: &str,
) -> Result<(), String> {
    let width = ZOOMED_WIDTH as f32;
    let height = ZOOMED_HEIGHT as f32;
    let mut start_entity: Option<String> = None;
    for press in 0..MAX_LAP_PRESSES {
        let snapshot = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
        let entity = snapshot
            .focused_entity
            .clone()
            .ok_or_else(|| format!("{screen}: ArrowRight left nothing focused (stop {press})"))?;
        match &start_entity {
            None => start_entity = Some(entity),
            Some(start) if *start == entity => return Ok(()),
            Some(_) => {}
        }
        // Poll for the post-scroll rect to land in view.
        let mut rect = snapshot.focused_rect;
        let mut visible = rect.is_some_and(|r| rect_in_viewport(&r, width, height));
        for _ in 0..30 {
            if visible {
                break;
            }
            checkpoint.wait_for_frame()?;
            if let Some(current) = read_accessibility(checkpoint)? {
                rect = current.focused_rect;
                visible = rect.is_some_and(|r| rect_in_viewport(&r, width, height));
            }
        }
        if !visible {
            return Err(format!(
                "{screen}: control {:?} (stop {press}) stays clipped outside the \
                 {width}x{height} viewport at 200% zoom even after scroll-into-view: {rect:?}",
                read_accessibility(checkpoint)?.and_then(|s| s.focused_label)
            ));
        }
    }
    Err(format!(
        "{screen}: tab order never wrapped back to the starting control within \
         {MAX_LAP_PRESSES} ArrowRight presses"
    ))
}

fn assert_screen_ok(checkpoint: &Checkpoint, screen: &str) -> Result<(), String> {
    wait_for_stable_frames(checkpoint)?;
    let status = checkpoint.read_status()?;
    check_no_unexpected_scroll(&status)?;
    assert_every_control_visible_when_focused(checkpoint, screen)?;
    if !status.errors.is_empty() {
        return Err(format!(
            "{screen}: page-level errors observed: {:?}",
            status.errors
        ));
    }
    let console_errors: Vec<&String> = status
        .console
        .iter()
        .filter(|line| line.starts_with("error:"))
        .collect();
    if !console_errors.is_empty() {
        return Err(format!(
            "{screen}: console.error observed: {console_errors:?}"
        ));
    }
    Ok(())
}

fn run_checks(
    server: &StaticServer,
    dir: &std::path::Path,
    profile_dir: &std::path::Path,
) -> Result<(), String> {
    let checkpoint = browser::launch(ZOOMED_WIDTH, ZOOMED_HEIGHT, 1.0, profile_dir)?;
    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url)?;

    wait_for_screen(&checkpoint, "MainMenu", true)?;
    assert_screen_ok(&checkpoint, "MainMenu")?;
    let menu_shot = checkpoint.screenshot_png(ZOOMED_WIDTH, ZOOMED_HEIGHT)?;
    let _ = artifacts::write_artifact(dir, "1-menu.png", &menu_shot);

    press_arrow_until_label(&checkpoint, "Setări")?;
    checkpoint.press_key("Enter")?;
    for _ in 0..SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(s) = read_accessibility(&checkpoint)?
            && s.focused_label
                .as_deref()
                .is_some_and(|l| l.starts_with("Muzică") || l == "-")
        {
            break;
        }
    }
    assert_screen_ok(&checkpoint, "Settings")?;
    let settings_shot = checkpoint.screenshot_png(ZOOMED_WIDTH, ZOOMED_HEIGHT)?;
    let _ = artifacts::write_artifact(dir, "2-settings.png", &settings_shot);
    press_arrow_until_label(&checkpoint, "Înapoi")?;
    checkpoint.press_key("Enter")?;
    for _ in 0..SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if read_screen(&checkpoint)?.as_deref() == Some("MainMenu") {
            break;
        }
    }

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
    // Screens with continuous idle animation (the creation/shop cutout
    // previews, the fight screen's parallax) can never satisfy
    // `assert_screen_ok`'s byte-identical-frames stability streak with the
    // clock running -- freeze `Time<Virtual>` around each check, the same
    // pause/capture pattern `gold_journey::captured_checkpoint` documents.
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )?;
    assert_screen_ok(&checkpoint, "CharacterCreation")?;
    let creation_shot = checkpoint.screenshot_png(ZOOMED_WIDTH, ZOOMED_HEIGHT)?;
    let _ = artifacts::write_artifact(dir, "3-creation.png", &creation_shot);
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    )?;

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
    )?;
    wait_for_screen(&checkpoint, "Fight", false)?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )?;
    assert_screen_ok(&checkpoint, "Fight")?;
    let fight_shot = checkpoint.screenshot_png(ZOOMED_WIDTH, ZOOMED_HEIGHT)?;
    let _ = artifacts::write_artifact(dir, "4-fight.png", &fight_shot);
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
    assert_screen_ok(&checkpoint, "FightResult")?;
    let result_shot = checkpoint.screenshot_png(ZOOMED_WIDTH, ZOOMED_HEIGHT)?;
    let _ = artifacts::write_artifact(dir, "5-fight-result.png", &result_shot);
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    )?;

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "GoToShop"}),
    )?;
    wait_for_screen(&checkpoint, "Shop", false)?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": true}),
    )?;
    assert_screen_ok(&checkpoint, "Shop")?;
    let shop_shot = checkpoint.screenshot_png(ZOOMED_WIDTH, ZOOMED_HEIGHT)?;
    let _ = artifacts::write_artifact(dir, "6-shop.png", &shop_shot);
    let _ = send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    );

    Ok(())
}
