//! The `fight-palette-desktop` scenario (#189, a child of #143): drives to a
//! real fight through the #187 review seam (`src/review/mod.rs`) and proves
//! the desktop action palette — the seven current combat actions, now
//! rendered entirely from [`crate::combat`]'s action-descriptor contract
//! (`combat::actions`/`combat::action_palette`) instead of a hard-coded
//! seven-button HUD — paints without overflow or clipping at a real desktop
//! viewport. Extends #168's harness per the documented extension pattern
//! (see `web_smoke::mod`'s module docs): a new module here plus one match
//! arm in `web_smoke::run_scenario`, reusing the review seam exactly the way
//! `gold_journey`/`reduced_motion_fight` already do.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test combat::actions --lib` / `cargo test combat::action_palette --lib`
//! prove the *descriptor generator* and the *ECS wiring* (spawn count,
//! enabled/disabled reconciliation, the extensibility seam) headlessly, with
//! no renderer at all. What only a real, freshly-launched browser proves:
//!
//! 1. The **actual measured layout** — real font metrics, a real
//!    `taffy`/`bevy_ui` flex pass, a real winit window at a real desktop
//!    size — places every one of the seven action buttons entirely inside
//!    the letterboxed stage rect, not just the arithmetic estimate
//!    `action_palette::tests::action_tiles_are_icon_led_and_fit_the_desktop_strip`
//!    already pins headlessly.
//! 2. The palette is reachable through the **same production path** a
//!    player uses: menu -> creation -> a real fight start, driven by the
//!    review seam's `pressButton`/`selectPreset` commands (which press the
//!    screens' actual button entities and run their actual production
//!    handlers — see `src/review/mod.rs`'s module docs).
//!
//! ## Reading an exact geometry fact instead of diffing screenshots
//!
//! A screenshot diff alone cannot distinguish "every button rendered inside
//! the stage rect" from "one button's right edge is a few pixels into the
//! letterbox pillarbar" — anti-aliasing and JPEG-free-but-still-lossy visual
//! comparison make that boundary hard to assert precisely. Instead, this
//! scenario reads [`crate::review`]'s small `PaletteSnapshot` telemetry
//! (mirrored here as a plain struct, the same duplicated-string-literal
//! convention `REVIEW_COMMAND_KEY`/`REVIEW_SCREEN_KEY` already use) —
//! computed once, in native Bevy UI space, from the *real* `ComputedNode`/
//! `UiGlobalTransform` of every `ActionButton` entity against the real
//! `LetterboxRect` — and asserts directly: exactly seven buttons exist, and
//! every one of them fits entirely inside the stage rect (`fits == true`).
//! The captured screenshot is still baselined (`--update-baselines`) for
//! human review, but the hard pass/fail gate is the exact telemetry fact,
//! not a pixel diff.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-fight-palette-desktop/` directory (mirroring
//! `gold_journey`/`reduced_motion_fight`) — never `dist/`, `dist-gold-journey/`,
//! nor `dist-reduced-motion-fight/`, so concurrent scenario runs never
//! clobber each other's build output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "fight-palette-desktop";

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 800;

/// `localStorage` key this scenario writes pending review commands to.
/// Mirrors `crate::review::REVIEW_COMMAND_KEY` in the *game* crate.
const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
/// `localStorage` key the game publishes the current screen's name to.
/// Mirrors `crate::review::REVIEW_SCREEN_KEY`.
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
/// `localStorage` key the game publishes a `PaletteSnapshot` to every frame
/// the fight HUD's action bar is up. Mirrors `crate::review::REVIEW_PALETTE_KEY`.
const REVIEW_PALETTE_KEY: &str = "rff_review_palette_v1";

/// Fixed combat seed, kept only for reproducibility (this scenario never
/// autoplays or resolves the duel — it captures the fresh fight-start
/// palette before either fighter has acted).
const FIGHT_PALETTE_SEED: u64 = 20;
/// `HeroPreset::Voinicul`'s exact display name (see `creation::draft::HeroPreset::name`).
const FIGHT_PALETTE_PRESET: &str = "Voinicul";

/// The seven current combat actions (`combat::actions::ALL_ACTIONS`); this
/// scenario asserts exactly this many buttons render, so a future action
/// added to that array (without also updating this constant) fails loudly
/// here instead of silently under-testing the palette.
const EXPECTED_BUTTON_COUNT: usize = 7;

const BASE_REQUIRED_ASSETS: &[(&str, &str)] = &[
    (
        "assets/fonts/Alegreya-Variable.ttf",
        "assets/fonts/Alegreya-Variable.ttf",
    ),
    ("assets/ui/panel_border.png", "assets/ui/panel_border.png"),
];

const READY_MAX_FRAMES: usize = 3600;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const STABLE_FRAMES_REQUIRED: usize = 3;

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-fight-palette-desktop/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-fight-palette-desktop/ at {}",
        server.base_url()
    );

    let profile_dir = artifacts::scenario_dir(SCENARIO).join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let mut missing_baseline = false;
    let result = run_checkpoint(
        &server,
        &profile_dir,
        update_baselines,
        strict_visual,
        &mut missing_baseline,
    );

    match result {
        Ok(()) if update_baselines => {
            println!(
                "\n{SCENARIO}: baseline updated at tests/visual/baselines/{SCENARIO}/desktop.png."
            );
            Ok(())
        }
        Ok(()) if missing_baseline => {
            println!(
                "\n{SCENARIO}: no accepted baseline existed yet -- the non-screenshot \
                 assertions above (seven buttons, all fitting inside the stage rect) still ran \
                 and passed. Re-run with --update-baselines once you've reviewed the captured \
                 screenshot to accept it."
            );
            Ok(())
        }
        Ok(()) => {
            println!("\n{SCENARIO}: matches the accepted baseline.");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn build_review_release() -> Result<PathBuf, SmokeError> {
    let mut cmd = Command::new("trunk");
    cmd.arg("build")
        .arg("--release")
        .arg("--features")
        .arg("review")
        .arg("--dist")
        .arg("dist-fight-palette-desktop");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (fight-palette-desktop)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-fight-palette-desktop"))
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
    for _ in 0..300 {
        checkpoint.wait_for_frame()?;
        let pending =
            checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_COMMAND_KEY}')"))?;
        if pending.is_none() {
            return Ok(());
        }
    }
    Err(format!(
        "review command was never consumed by the game within 300 frames: {json}"
    ))
}

fn read_screen(checkpoint: &Checkpoint) -> Result<Option<String>, String> {
    checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_SCREEN_KEY}')"))
}

/// Mirrors `crate::review::PaletteSnapshot`.
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
struct PaletteSnapshot {
    button_count: usize,
    fits: bool,
}

fn read_palette(checkpoint: &Checkpoint) -> Result<Option<PaletteSnapshot>, String> {
    let raw = checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_PALETTE_KEY}')"))?;
    match raw {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("palette snapshot was not valid JSON ({json}): {e}")),
    }
}

struct Readiness {
    reached_screen: bool,
    stabilized: bool,
    frames_observed: usize,
    elapsed: Duration,
    last_screen: Option<String>,
}

/// Waits for the `Fight` screen (per the review seam's readiness contract)
/// and #168's byte-identical-frames stability streak, freezing
/// `Time<Virtual>` around the capture so the fight screen's idle parallax
/// sway can't defeat the streak (mirrors `gold_journey::captured_checkpoint`).
fn run_checkpoint(
    server: &StaticServer,
    profile_dir: &std::path::Path,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let dir = artifacts::checkpoint_dir(SCENARIO, "desktop").map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    let outcome = (|| -> Result<(PageStatus, Vec<u8>, Readiness, PaletteSnapshot), String> {
        // DPR 1 always -- this scenario is unchanged by #198's DPR matrix,
        // which extends `gold-journey` instead (see that module's docs).
        let checkpoint = browser::launch(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, 1.0, profile_dir)?;
        let url = format!("{}/", server.base_url());
        checkpoint.navigate(&url)?;

        wait_for_screen(&checkpoint, "MainMenu", true)?;
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "seedCombat", "seed": FIGHT_PALETTE_SEED}),
        )?;
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
        )?;
        wait_for_screen(&checkpoint, "CharacterCreation", false)?;
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "selectPreset", "preset": FIGHT_PALETTE_PRESET}),
        )?;
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
        )?;

        // Freeze the clock before the capture so the fight screen's
        // continuous idle animation (parallax drift, sprite bob) can't
        // defeat the byte-identical stability streak.
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "setTimePaused", "paused": true}),
        )?;
        let (status, screenshot, readiness) = wait_for_readiness(&checkpoint)?;
        let palette = read_palette(&checkpoint)?.ok_or_else(|| {
            format!(
                "never observed a palette snapshot under {REVIEW_PALETTE_KEY:?} on the fight \
                 screen -- review::publish_palette_state may not be wired up"
            )
        })?;
        // Best-effort unpause even though this scenario exits right after --
        // mirrors `gold_journey`'s "never leave the clock frozen" discipline
        // for a debugging session against the still-open browser.
        let _ = send_command(
            &checkpoint,
            serde_json::json!({"cmd": "setTimePaused", "paused": false}),
        );

        Ok((status, screenshot, readiness, palette))
    })();

    let (status, screenshot, readiness, palette) = match outcome {
        Ok(quad) => quad,
        Err(message) => {
            let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
            return Err(SmokeError::scenario(
                format!("web-smoke {SCENARIO}"),
                message,
                dir,
            ));
        }
    };

    write_artifacts(&dir, &status, &screenshot, server, &palette);

    let mut problems = Vec::new();
    if !readiness.reached_screen {
        problems.push(format!(
            "never observed screen `Fight` within {READY_MAX_WALL_CLOCK:?}/{READY_MAX_FRAMES} \
             frames (last seen: {:?})",
            readiness.last_screen
        ));
    } else if !readiness.stabilized {
        problems.push(format!(
            "first paint never stabilized on the fight screen within \
             {READY_MAX_WALL_CLOCK:?}/{READY_MAX_FRAMES} frames ({} observed)",
            readiness.frames_observed
        ));
    } else {
        check_no_console_or_page_errors(&status, &mut problems);
        check_required_assets(&status, &mut problems);
        check_no_unexpected_scroll(&status, &mut problems);
        check_screenshot_pixels(&screenshot, &mut problems);
        check_palette(&palette, &mut problems);
    }

    if !problems.is_empty() {
        let message = format!(
            "{SCENARIO} ({VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}, ready in {:?}, {} frame(s)) failed:\n  - {}",
            readiness.elapsed,
            readiness.frames_observed,
            problems.join("\n  - ")
        );
        return Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}"),
            message,
            dir,
        ));
    }

    match baseline::handle(SCENARIO, "desktop", &screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => {
            println!(
                "{SCENARIO}: OK -- baseline updated -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            println!(
                "{SCENARIO}: OK -- no baseline exists yet -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Matches) => {
            println!(
                "{SCENARIO}: OK -- matches accepted baseline -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Differs {
            diff_pixels,
            total_pixels,
        }) => {
            let diff_paths = baseline::write_diff_triplet(SCENARIO, "desktop", &screenshot, &dir);
            if strict_visual {
                let mut message = format!(
                    "{SCENARIO} ({VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}) failed:\n  - screenshot \
                     differs from accepted baseline ({diff_pixels}/{total_pixels} px) under \
                     --strict-visual"
                );
                if let Ok(paths) = &diff_paths {
                    message.push_str(&format!("\n  diff triplet: {}", paths.describe()));
                }
                return Err(SmokeError::scenario(
                    format!("web-smoke {SCENARIO}"),
                    message,
                    dir,
                ));
            }
            println!(
                "{SCENARIO}: OK -- differs from accepted baseline ({diff_pixels}/{total_pixels} px; \
                 not a scenario failure by itself unless --strict-visual, see baseline.rs docs) -- artifacts: {}",
                dir.display()
            );
        }
        Err(e) => {
            println!("{SCENARIO}: WARNING -- baseline comparison failed to run: {e}");
        }
    }

    Ok(())
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

/// Waits for the `Fight` screen and #168's screenshot-stability streak,
/// exactly like `gold_journey::wait_for_readiness` keyed on the review
/// seam's screen marker.
fn wait_for_readiness(checkpoint: &Checkpoint) -> Result<(PageStatus, Vec<u8>, Readiness), String> {
    let start = Instant::now();
    let mut last_status: Option<PageStatus> = None;
    let mut last_screenshot: Option<Vec<u8>> = None;
    let mut last_screen: Option<String> = None;
    let mut stable_count = 0usize;
    let mut frames_observed = 0usize;

    for _ in 0..READY_MAX_FRAMES {
        if start.elapsed() > READY_MAX_WALL_CLOCK {
            break;
        }
        checkpoint.wait_for_frame()?;
        frames_observed += 1;
        let screen = read_screen(checkpoint)?;
        last_screen = screen.clone();
        let status = checkpoint.read_status()?;

        let ready_screen = status.app_booted() && screen.as_deref() == Some("Fight");
        if !ready_screen {
            stable_count = 0;
            last_screenshot = None;
            last_status = Some(status);
            continue;
        }

        let screenshot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
        if last_screenshot.as_deref() == Some(screenshot.as_slice()) {
            stable_count += 1;
        } else {
            stable_count = 1;
        }
        last_screenshot = Some(screenshot);
        last_status = Some(status);

        if stable_count >= STABLE_FRAMES_REQUIRED {
            return Ok((
                last_status.expect("just set"),
                last_screenshot.expect("just set"),
                Readiness {
                    reached_screen: true,
                    stabilized: true,
                    frames_observed,
                    elapsed: start.elapsed(),
                    last_screen,
                },
            ));
        }
    }

    let reached_screen = last_screen.as_deref() == Some("Fight");
    let status = match last_status {
        Some(status) => status,
        None => checkpoint.read_status()?,
    };
    let screenshot = match last_screenshot {
        Some(shot) => shot,
        None => checkpoint
            .screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
            .unwrap_or_default(),
    };
    Ok((
        status,
        screenshot,
        Readiness {
            reached_screen,
            stabilized: false,
            frames_observed,
            elapsed: start.elapsed(),
            last_screen,
        },
    ))
}

fn write_artifacts(
    dir: &std::path::Path,
    status: &PageStatus,
    screenshot: &[u8],
    server: &StaticServer,
    palette: &PaletteSnapshot,
) {
    let _ = artifacts::write_artifact(dir, "screenshot.png", screenshot);
    let _ = artifacts::write_artifact(dir, "console.log", status.console.join("\n"));
    let _ = artifacts::write_artifact(
        dir,
        "network.log",
        status
            .resources
            .iter()
            .map(|r| format!("{} {} ({} bytes)", r.status, r.url, r.transfer_size as u64))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let _ = artifacts::write_artifact(
        dir,
        "viewport.log",
        format!(
            "requested: {VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}\nmeasured inner: {}x{}\nmeasured client: {}x{}\nscroll: {}x{}\ndevicePixelRatio: {}\ncanvas backing size: {}x{}\nerrors: {:?}\n",
            status.inner_width,
            status.inner_height,
            status.client_width,
            status.client_height,
            status.scroll_width,
            status.scroll_height,
            status.device_pixel_ratio,
            status.canvas_w,
            status.canvas_h,
            status.errors,
        ),
    );
    let _ = artifacts::write_artifact(
        dir,
        "palette.log",
        format!(
            "button_count: {} (expected {EXPECTED_BUTTON_COUNT})\nfits within stage rect: {}\n",
            palette.button_count, palette.fits
        ),
    );
    let _ = artifacts::write_artifact(dir, "server.log", server.request_log().join("\n"));
}

fn check_no_console_or_page_errors(status: &PageStatus, problems: &mut Vec<String>) {
    if !status.errors.is_empty() {
        problems.push(format!("page-level errors observed: {:?}", status.errors));
    }
    let console_errors: Vec<&String> = status
        .console
        .iter()
        .filter(|line| line.starts_with("error:"))
        .collect();
    if !console_errors.is_empty() {
        problems.push(format!("console.error observed: {console_errors:?}"));
    }
}

fn check_required_assets(status: &PageStatus, problems: &mut Vec<String>) {
    for (suffix, _source) in BASE_REQUIRED_ASSETS {
        let matching = status.resources.iter().find(|r| r.url.ends_with(*suffix));
        match matching {
            None => problems.push(format!("required asset never fetched: {suffix}")),
            Some(entry) if !(200..300).contains(&entry.status) => problems.push(format!(
                "required asset {suffix} fetched with non-success status {}",
                entry.status
            )),
            Some(entry) if entry.transfer_size <= 0.0 => problems.push(format!(
                "required asset {suffix} fetched but empty (0 bytes)"
            )),
            Some(_) => {}
        }
    }
}

fn check_no_unexpected_scroll(status: &PageStatus, problems: &mut Vec<String>) {
    const EPSILON: f64 = 1.0;
    if (status.inner_width - f64::from(VIEWPORT_WIDTH)).abs() > EPSILON
        || (status.inner_height - f64::from(VIEWPORT_HEIGHT)).abs() > EPSILON
    {
        problems.push(format!(
            "viewport is {}x{}, expected exactly {VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT} \
             (device-metrics override did not take)",
            status.inner_width, status.inner_height
        ));
    }
    if status.scroll_width > status.client_width + EPSILON {
        problems.push(format!(
            "document scrolls horizontally: scrollWidth {} > clientWidth {}",
            status.scroll_width, status.client_width
        ));
    }
    if status.scroll_height > status.client_height + EPSILON {
        problems.push(format!(
            "document scrolls vertically: scrollHeight {} > clientHeight {}",
            status.scroll_height, status.client_height
        ));
    }
    if (status.device_pixel_ratio - 1.0).abs() > f64::EPSILON {
        problems.push(format!(
            "devicePixelRatio was {} (expected 1)",
            status.device_pixel_ratio
        ));
    }
}

fn check_screenshot_pixels(screenshot_png: &[u8], problems: &mut Vec<String>) {
    let image = match image::load_from_memory(screenshot_png) {
        Ok(image) => image,
        Err(e) => {
            problems.push(format!("captured screenshot was not a decodable PNG: {e}"));
            return;
        }
    };
    if image.width() != VIEWPORT_WIDTH || image.height() != VIEWPORT_HEIGHT {
        problems.push(format!(
            "screenshot was {}x{}, expected {VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT} (DPR 1)",
            image.width(),
            image.height(),
        ));
        return;
    }

    let rgba = image.to_rgba8();
    let mut min = [255u8; 3];
    let mut max = [0u8; 3];
    for pixel in rgba.pixels() {
        for channel in 0..3 {
            min[channel] = min[channel].min(pixel[channel]);
            max[channel] = max[channel].max(pixel[channel]);
        }
    }
    let variance = max
        .iter()
        .zip(min.iter())
        .map(|(a, b)| a.saturating_sub(*b))
        .max()
        .unwrap_or(0);
    if variance < 24 {
        problems.push(format!(
            "screenshot is nearly a single flat color (max channel range {variance}/255) -- \
             the fight screen likely never painted (blank canvas)"
        ));
    }
}

/// The exact geometry gate this scenario exists for: every button rendered,
/// none clipped or overflowing the letterboxed stage rect.
fn check_palette(palette: &PaletteSnapshot, problems: &mut Vec<String>) {
    if palette.button_count != EXPECTED_BUTTON_COUNT {
        problems.push(format!(
            "expected {EXPECTED_BUTTON_COUNT} action buttons, the palette snapshot reports {} \
             -- either a button failed to render or `combat::actions::ALL_ACTIONS` grew without \
             updating this scenario's EXPECTED_BUTTON_COUNT",
            palette.button_count
        ));
    }
    if !palette.fits {
        problems.push(
            "the action bar overflows or clips the letterboxed stage rect -- at least one \
             action button's rendered box extends outside `LetterboxRect` (see \
             `review::publish_palette_state`)"
                .to_string(),
        );
    }
}
