//! The `touch-targets` scenario (#216, a child of #145): drives a real,
//! freshly-launched browser through every current screen at both a desktop
//! and a phone viewport and asserts no currently-visible interactive control
//! is smaller than 44x44 CSS pixels. Extends #168's harness per the
//! documented extension pattern (see `web_smoke::mod`'s module docs): a new
//! module here plus one match arm in `web_smoke::run_scenario`.
//!
//! ## Why this cannot be a screenshot pixel scan
//!
//! The whole UI is `bevy_ui` canvas/WebGL output -- there is no DOM element
//! per button (see `accessibility_settings_reload`'s module docs for the
//! same limitation applied to button-color scanning). `src/review/mod.rs`'s
//! `AccessibilitySnapshot` (#216) instead reads every currently-visible
//! `Focusable` control's actual on-screen box straight from its live
//! `ComputedNode`/`UiGlobalTransform` (the same native-Bevy-space computation
//! `PaletteSnapshot.fits` already uses for the fight palette), in the same
//! logical (CSS) pixel unit the 44px floor is expressed in -- exact ground
//! truth, not a pixel-color guess.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-touch-targets/` directory, mirroring every other
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
use crate::web_smoke::browser::{self, Checkpoint};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "touch-targets";

const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
const REVIEW_ACCESSIBILITY_KEY: &str = "rff_review_a11y_v1";

/// The 44px CSS-pixel floor the issue requires (mirrors
/// `crate::theme::MIN_TOUCH_TARGET`).
const MIN_TOUCH_TARGET: f32 = 44.0;

const KEYBOARD_ACCESSIBILITY_SEED: u64 = 21;
const KEYBOARD_ACCESSIBILITY_PRESET: &str = "Voinicul";

const READY_MAX_FRAMES: usize = 3600;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const STABLE_FRAMES_REQUIRED: usize = 3;
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;
const SETTLE_MAX_FRAMES: usize = 120;

struct ViewportSpec {
    name: &'static str,
    width: u32,
    height: u32,
}

const VIEWPORTS: &[ViewportSpec] = &[
    ViewportSpec {
        name: "desktop",
        width: 1280,
        height: 800,
    },
    ViewportSpec {
        name: "phone",
        width: 390,
        height: 844,
    },
];

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (see its module docs)."
        );
    }

    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-touch-targets/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-touch-targets/ at {}",
        server.base_url()
    );

    for viewport in VIEWPORTS {
        run_viewport(viewport, &server)?;
    }

    println!(
        "\n{SCENARIO}: no interactive control was found below {MIN_TOUCH_TARGET}x{MIN_TOUCH_TARGET} \
         CSS px at any viewport."
    );
    Ok(())
}

fn build_review_release() -> Result<PathBuf, SmokeError> {
    let mut cmd = Command::new("trunk");
    cmd.arg("build")
        .arg("--release")
        .arg("--features")
        .arg("review")
        .arg("--dist")
        .arg("dist-touch-targets");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (touch-targets)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-touch-targets"))
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
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// Mirrors `crate::review::AccessibilitySnapshot` (#216).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct AccessibilitySnapshot {
    focused_label: Option<String>,
    #[allow(dead_code)]
    focus_marker_visible: bool,
    targets: Vec<TargetRect>,
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

fn wait_for_stable_frames(checkpoint: &Checkpoint, width: u32, height: u32) -> Result<(), String> {
    let mut last: Option<Vec<u8>> = None;
    let mut stable = 0usize;
    for _ in 0..600 {
        checkpoint.wait_for_frame()?;
        let shot = checkpoint.screenshot_png(width, height)?;
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

/// Reads the current accessibility snapshot and asserts every visible
/// target meets [`MIN_TOUCH_TARGET`], listing every offending target's exact
/// box on failure.
fn assert_targets_meet_floor(checkpoint: &Checkpoint, screen: &str) -> Result<(), String> {
    let mut snapshot = None;
    for _ in 0..SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(s) = read_accessibility(checkpoint)? {
            snapshot = Some(s);
            break;
        }
    }
    let snapshot = snapshot
        .ok_or_else(|| format!("{screen}: no accessibility snapshot was ever published"))?;
    if snapshot.targets.is_empty() {
        return Err(format!(
            "{screen}: no interactive targets were reported at all -- the snapshot is likely \
             broken, not the screen"
        ));
    }
    if snapshot.min_target_size < MIN_TOUCH_TARGET {
        let offenders: Vec<&TargetRect> = snapshot
            .targets
            .iter()
            .filter(|t| t.width.min(t.height) < MIN_TOUCH_TARGET)
            .collect();
        return Err(format!(
            "{screen}: {} interactive target(s) below {MIN_TOUCH_TARGET}x{MIN_TOUCH_TARGET} CSS \
             px: {offenders:?}",
            offenders.len()
        ));
    }
    Ok(())
}

fn run_viewport(viewport: &ViewportSpec, server: &StaticServer) -> Result<(), SmokeError> {
    let dir = artifacts::checkpoint_dir(SCENARIO, viewport.name).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{}]", viewport.name),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    let profile_dir =
        artifacts::scenario_dir(SCENARIO).join(format!("chrome-profile-{}", viewport.name));
    let _ = std::fs::remove_dir_all(&profile_dir);

    let outcome = run_checks(viewport, server, &dir, &profile_dir);
    let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));

    match outcome {
        Ok(()) => {
            println!(
                "{SCENARIO}[{}]: OK -- every visible target meets the {MIN_TOUCH_TARGET}px floor \
                 -- artifacts: {}",
                viewport.name,
                dir.display()
            );
            Ok(())
        }
        Err(message) => Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{}]", viewport.name),
            message,
            dir,
        )),
    }
}

fn run_checks(
    viewport: &ViewportSpec,
    server: &StaticServer,
    dir: &std::path::Path,
    profile_dir: &std::path::Path,
) -> Result<(), String> {
    let checkpoint = browser::launch(viewport.width, viewport.height, 1.0, profile_dir)?;
    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url)?;

    wait_for_screen(&checkpoint, "MainMenu", true)?;
    wait_for_stable_frames(&checkpoint, viewport.width, viewport.height)?;
    assert_targets_meet_floor(&checkpoint, "MainMenu")?;
    let menu_shot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
    let _ = artifacts::write_artifact(dir, "1-menu.png", &menu_shot);

    // Settings, opened for real over the menu with the same keyboard path
    // `keyboard-accessibility` uses (no DOM/pixel-color click needed).
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
    assert_targets_meet_floor(&checkpoint, "Settings")?;
    let settings_shot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
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
    wait_for_stable_frames(&checkpoint, viewport.width, viewport.height)?;
    assert_targets_meet_floor(&checkpoint, "CharacterCreation")?;
    let creation_shot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
    let _ = artifacts::write_artifact(dir, "3-creation.png", &creation_shot);

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
    )?;
    wait_for_screen(&checkpoint, "Fight", false)?;
    wait_for_stable_frames(&checkpoint, viewport.width, viewport.height)?;
    assert_targets_meet_floor(&checkpoint, "Fight")?;
    let fight_shot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
    let _ = artifacts::write_artifact(dir, "4-fight.png", &fight_shot);

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
    )?;
    wait_for_screen(&checkpoint, "FightResult", false)?;
    wait_for_stable_frames(&checkpoint, viewport.width, viewport.height)?;
    assert_targets_meet_floor(&checkpoint, "FightResult")?;
    let result_shot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
    let _ = artifacts::write_artifact(dir, "5-fight-result.png", &result_shot);

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "GoToShop"}),
    )?;
    wait_for_screen(&checkpoint, "Shop", false)?;
    wait_for_stable_frames(&checkpoint, viewport.width, viewport.height)?;
    assert_targets_meet_floor(&checkpoint, "Shop")?;
    let shop_shot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
    let _ = artifacts::write_artifact(dir, "6-shop.png", &shop_shot);

    let status = checkpoint.read_status()?;
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
    let _ = artifacts::write_artifact(dir, "console.log", status.console.join("\n"));

    Ok(())
}
