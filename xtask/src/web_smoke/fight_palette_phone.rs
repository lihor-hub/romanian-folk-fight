//! The `fight-palette-phone` scenario (#199, a child of #143; extended by
//! #276): drives to a real fight through the #187 review seam
//! (`src/review/mod.rs`) at a phone viewport (390x844, DPR 1) and proves the
//! category-disclosure combat palette — at most four primary category
//! controls, each opening exactly its descriptors' actions — lands inside
//! the real browser window with every touch target at least 44x44 CSS px
//! *and never covers either fighter's readable body area or the combat log*,
//! in the closed state *and* every open-category state. Extends #168's
//! harness per the documented extension pattern (see `web_smoke::mod`'s
//! module docs): a new module here plus one match arm in
//! `web_smoke::run_scenario`, reusing the review seam exactly the way
//! `fight_palette_desktop`/`gold_journey` already do.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test combat::action_palette --lib` proves the grouping,
//! open/close/switch toggling, duel-state preservation, and the declared
//! `Node` minimums (including the #276 `phone_bar_bottom_offset` contract)
//! headlessly. What only a real, freshly-launched browser proves: the
//! **actual measured layout** — real font metrics (Romanian labels like
//! "Mișcare" at real glyph widths), a real `taffy`/`bevy_ui` flex pass, a
//! real 390x844 winit canvas — keeps at most four categories visible, every
//! rendered target at ≥44 CSS px, and every open state's buttons entirely
//! inside the real window *and* clear of the fighters/combat log, reachable
//! through the same production path a player uses (menu -> creation -> a
//! real fight start).
//!
//! ## Reading exact geometry facts instead of diffing screenshots
//!
//! Like `fight-palette-desktop` (#189), the hard pass/fail gate is
//! `review::publish_palette_state`'s `PaletteSnapshot` telemetry (mirrored
//! below as plain structs, the same duplicated-string-literal convention the
//! other scenarios use), extended for #199 with a `phone` object: visible
//! category count, the open category id, the open action ids, every visible
//! target's on-screen box, a fits-in-window bit, and the minimum target
//! dimension — plus, since #276, whether any visible control intersects
//! either fighter's deterministic readable-body-region proxy
//! (`review::fighter_readable_rect`) or `hud::LogPanelRoot`'s rendered rect —
//! all computed once in native Bevy UI space from real
//! `ComputedNode`/`UiGlobalTransform` values. Categories are opened through
//! the seam's `pressActionCategory` command, which presses the *real*
//! `CategoryButton` entity (the same production toggle a tap runs), so the
//! open states this scenario captures are the exact states a player reaches.
//! Screenshots of the closed state and every open state are still baselined
//! (`--update-baselines`) for human review, but pixels never gate.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-fight-palette-phone/` directory (mirroring the other
//! review-seam scenarios) so concurrent scenario runs never clobber each
//! other's build output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "fight-palette-phone";

const VIEWPORT_WIDTH: u32 = 390;
const VIEWPORT_HEIGHT: u32 = 844;

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

/// The issue's touch-target floor, in CSS px (DPR 1 is forced by
/// `browser::launch`, so logical == CSS px here).
const MIN_TOUCH_TARGET: f32 = 44.0;

/// #199's acceptance criterion: never more than four primary category
/// controls at once.
const MAX_CATEGORY_CONTROLS: usize = 4;

/// Every category the seven current actions span, in
/// `combat::actions::CATEGORY_ORDER` order, with the exact sorted action ids
/// its disclosure must reveal (`combat::actions::action_category`'s
/// membership). A future action added to `combat::actions::ALL_ACTIONS`
/// (without also updating this table) fails loudly here instead of silently
/// under-testing the palette — the same pinning convention
/// `fight_palette_desktop::EXPECTED_BUTTON_COUNT` uses.
const EXPECTED_CATEGORIES: &[(&str, &[&str])] = &[
    ("strikes", &["heavy-strike", "quick-strike"]),
    ("defense", &["block"]),
    ("movement", &["leap-forward", "step-back", "step-forward"]),
    ("utility", &["rest"]),
];

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
/// Frame budget for one palette-state change (category open/close/switch)
/// to be reflected in the published snapshot after its command is consumed.
const PALETTE_STATE_MAX_FRAMES: usize = 600;

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-fight-palette-phone/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-fight-palette-phone/ at {}",
        server.base_url()
    );

    let profile_dir = artifacts::scenario_dir(SCENARIO).join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let mut missing_baseline = false;
    let result = run_journey(
        &server,
        &profile_dir,
        update_baselines,
        strict_visual,
        &mut missing_baseline,
    );

    match result {
        Ok(()) if update_baselines => {
            println!(
                "\n{SCENARIO}: baselines updated at tests/visual/baselines/{SCENARIO}/ for {} \
                 state(s).",
                EXPECTED_CATEGORIES.len() + 1
            );
            Ok(())
        }
        Ok(()) if missing_baseline => {
            println!(
                "\n{SCENARIO}: no accepted baseline existed yet for one or more states -- the \
                 non-screenshot assertions above (≤4 categories, registered actions only, every \
                 target ≥44px, everything inside the stage rect) still ran and passed. Re-run \
                 with --update-baselines once you've reviewed the captured screenshots to \
                 accept them."
            );
            Ok(())
        }
        Ok(()) => {
            println!("\n{SCENARIO}: all states passed against their accepted baselines.");
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
        .arg("dist-fight-palette-phone");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (fight-palette-phone)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-fight-palette-phone"))
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

/// One visible target's on-screen box in CSS px. Mirrors
/// `crate::review::TargetRect`.
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
struct TargetRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// Mirrors `crate::review::PhonePaletteSnapshot` (#199, extended by #276).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct PhonePaletteSnapshot {
    visible_category_count: usize,
    open_category: Option<String>,
    open_action_ids: Vec<String>,
    targets: Vec<TargetRect>,
    fits_in_window: bool,
    min_target_size: f32,
    overlaps_status_panels: bool,
    overlaps_fighter_region: bool,
    overlaps_log_panel: bool,
}

/// Mirrors `crate::review::PaletteSnapshot`.
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct PaletteSnapshot {
    button_count: usize,
    fits: bool,
    phone: Option<PhonePaletteSnapshot>,
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

/// Drives menu -> creation -> a fresh fight start at 390x844 (mirroring
/// `fight_palette_desktop::run_checkpoint`'s journey), then captures and
/// asserts the closed state plus every open-category state in one
/// continuous, time-frozen browser session.
fn run_journey(
    server: &StaticServer,
    profile_dir: &std::path::Path,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let journey = (|| -> Result<Checkpoint, String> {
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
        // Freeze the clock for the whole capture sequence: category
        // disclosure is input-driven (never time-driven), so open/close
        // still works while idle animation (parallax drift, sprite bob)
        // holds still for the byte-identical stability streaks.
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "setTimePaused", "paused": true}),
        )?;
        Ok(checkpoint)
    })();

    let checkpoint = match journey {
        Ok(checkpoint) => checkpoint,
        Err(message) => {
            let dir = artifacts::scenario_dir(SCENARIO);
            return Err(SmokeError::scenario(
                format!("web-smoke {SCENARIO}"),
                message,
                dir,
            ));
        }
    };

    // Closed state: wait for the fight screen + stability, then assert the
    // category row's geometry facts with nothing open.
    capture_state(
        &checkpoint,
        server,
        "closed",
        None,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    // Every open-category state, in the palette's own display order. Each
    // press switches directly from the previous category (the closed ->
    // strikes press opens; every later press is an open -> open switch), so
    // this also exercises the switch path end-to-end in a real browser.
    for (category, expected_actions) in EXPECTED_CATEGORIES {
        let press = send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressActionCategory", "category": *category}),
        );
        if let Err(message) = press {
            return Err(SmokeError::scenario(
                format!("web-smoke {SCENARIO}[open-{category}]"),
                message,
                artifacts::scenario_dir(SCENARIO),
            ));
        }
        capture_state(
            &checkpoint,
            server,
            &format!("open-{category}"),
            Some((category, expected_actions)),
            update_baselines,
            strict_visual,
            missing_baseline,
        )?;
    }

    // Best-effort unpause -- mirrors `gold_journey`'s "never leave the clock
    // frozen" discipline for a debugging session against the still-open
    // browser.
    let _ = send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setTimePaused", "paused": false}),
    );

    Ok(())
}

/// Waits until the published palette snapshot reflects `expected_open`
/// (`None` = closed), then for the screenshot-stability streak, captures,
/// writes artifacts, and runs every assertion for this state.
fn capture_state(
    checkpoint: &Checkpoint,
    server: &StaticServer,
    state_name: &str,
    expected_open: Option<(&str, &[&str])>,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let dir = artifacts::checkpoint_dir(SCENARIO, state_name).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{state_name}]"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    let outcome = wait_for_readiness(checkpoint, expected_open.map(|(category, _)| category));
    let (status, screenshot, readiness, palette) = match outcome {
        Ok(quad) => quad,
        Err(message) => {
            let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
            return Err(SmokeError::scenario(
                format!("web-smoke {SCENARIO}[{state_name}]"),
                message,
                dir,
            ));
        }
    };

    write_artifacts(&dir, &status, &screenshot, server, &palette);

    let mut problems = Vec::new();
    if !readiness.reached_screen {
        problems.push(format!(
            "never observed screen `Fight` with the expected palette state within \
             {READY_MAX_WALL_CLOCK:?}/{READY_MAX_FRAMES} frames (last seen: {:?})",
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
        check_palette(&palette, expected_open, &mut problems);
    }

    if !problems.is_empty() {
        let message = format!(
            "{SCENARIO}[{state_name}] ({VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}, ready in {:?}, {} \
             frame(s)) failed:\n  - {}",
            readiness.elapsed,
            readiness.frames_observed,
            problems.join("\n  - ")
        );
        return Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{state_name}]"),
            message,
            dir,
        ));
    }

    match baseline::handle(SCENARIO, state_name, &screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => {
            println!(
                "{SCENARIO}[{state_name}]: OK -- baseline updated -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            println!(
                "{SCENARIO}[{state_name}]: OK -- no baseline exists yet -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Matches) => {
            println!(
                "{SCENARIO}[{state_name}]: OK -- matches accepted baseline -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Differs {
            diff_pixels,
            total_pixels,
        }) => {
            let diff_paths = baseline::write_diff_triplet(SCENARIO, state_name, &screenshot, &dir);
            if strict_visual {
                let mut message = format!(
                    "{SCENARIO}[{state_name}] failed:\n  - screenshot differs from accepted \
                     baseline ({diff_pixels}/{total_pixels} px) under --strict-visual"
                );
                if let Ok(paths) = &diff_paths {
                    message.push_str(&format!("\n  diff triplet: {}", paths.describe()));
                }
                return Err(SmokeError::scenario(
                    format!("web-smoke {SCENARIO}[{state_name}]"),
                    message,
                    dir,
                ));
            }
            println!(
                "{SCENARIO}[{state_name}]: OK -- differs from accepted baseline \
                 ({diff_pixels}/{total_pixels} px; not a scenario failure by itself unless \
                 --strict-visual, see baseline.rs docs) -- artifacts: {}",
                dir.display()
            );
        }
        Err(e) => {
            println!("{SCENARIO}[{state_name}]: WARNING -- baseline comparison failed to run: {e}");
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

/// Whether the published snapshot reflects the palette state this capture
/// is waiting for: `None` = closed (no open category), `Some(id)` = that
/// category reported open. A missing snapshot or missing `phone` object is
/// "not yet".
fn palette_state_matches(palette: &Option<PaletteSnapshot>, expected_open: Option<&str>) -> bool {
    let Some(palette) = palette else {
        return false;
    };
    let Some(phone) = &palette.phone else {
        return false;
    };
    phone.open_category.as_deref() == expected_open
}

/// Waits for the `Fight` screen, the palette snapshot to report
/// `expected_open`, and #168's byte-identical-frames stability streak --
/// the same shape as `fight_palette_desktop::wait_for_readiness` with the
/// palette-state condition folded into readiness (a capture must never race
/// the open/close animation of a previous state).
#[allow(clippy::type_complexity)]
fn wait_for_readiness(
    checkpoint: &Checkpoint,
    expected_open: Option<&str>,
) -> Result<(PageStatus, Vec<u8>, Readiness, PaletteSnapshot), String> {
    let start = Instant::now();
    let mut last_status: Option<PageStatus> = None;
    let mut last_screenshot: Option<Vec<u8>> = None;
    let mut last_screen: Option<String> = None;
    let mut last_palette: Option<PaletteSnapshot> = None;
    let mut stable_count = 0usize;
    let mut frames_observed = 0usize;
    let mut state_frames = 0usize;

    for _ in 0..READY_MAX_FRAMES {
        if start.elapsed() > READY_MAX_WALL_CLOCK {
            break;
        }
        checkpoint.wait_for_frame()?;
        frames_observed += 1;
        let screen = read_screen(checkpoint)?;
        last_screen = screen.clone();
        let status = checkpoint.read_status()?;
        let palette = read_palette(checkpoint)?;
        let state_ready = palette_state_matches(&palette, expected_open);
        last_palette = palette;

        let ready_screen = status.app_booted() && screen.as_deref() == Some("Fight");
        if !ready_screen || !state_ready {
            state_frames += 1;
            if state_frames > PALETTE_STATE_MAX_FRAMES && ready_screen {
                return Err(format!(
                    "the palette snapshot never reported open_category == {expected_open:?} \
                     within {PALETTE_STATE_MAX_FRAMES} frames on the fight screen \
                     (last snapshot: {last_palette:?})"
                ));
            }
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
            let palette = last_palette
                .clone()
                .expect("state_ready implies a snapshot");
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
                palette,
            ));
        }
    }

    let reached_screen = last_screen.as_deref() == Some("Fight")
        && palette_state_matches(&last_palette, expected_open);
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
    let palette = last_palette.unwrap_or(PaletteSnapshot {
        button_count: 0,
        fits: false,
        phone: None,
    });
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
        palette,
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
    let phone_lines = match &palette.phone {
        Some(phone) => {
            let targets = phone
                .targets
                .iter()
                .map(|t| format!("  {:.1},{:.1} {:.1}x{:.1}", t.x, t.y, t.width, t.height))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "visible_category_count: {}\nopen_category: {:?}\nopen_action_ids: {:?}\nfits_in_window: {}\nmin_target_size: {:.1} (floor {MIN_TOUCH_TARGET})\noverlaps_status_panels: {}\noverlaps_fighter_region: {}\noverlaps_log_panel: {}\ntargets:\n{targets}\n",
                phone.visible_category_count,
                phone.open_category,
                phone.open_action_ids,
                phone.fits_in_window,
                phone.min_target_size,
                phone.overlaps_status_panels,
                phone.overlaps_fighter_region,
                phone.overlaps_log_panel,
            )
        }
        None => "phone: none (snapshot reported a desktop viewport?)\n".to_string(),
    };
    let _ = artifacts::write_artifact(
        dir,
        "palette.log",
        format!(
            "button_count: {}\nfits within stage rect: {}\n{phone_lines}",
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

/// The exact geometry gates this scenario exists for (#199's acceptance
/// criteria), asserted from the published telemetry for one palette state.
fn check_palette(
    palette: &PaletteSnapshot,
    expected_open: Option<(&str, &[&str])>,
    problems: &mut Vec<String>,
) {
    let Some(phone) = &palette.phone else {
        problems.push(
            "the palette snapshot carried no phone facts -- the game did not consider \
             390x844 a mobile viewport (`ViewportInfo.is_mobile`), or \
             `review::publish_palette_state` is not publishing them"
                .to_string(),
        );
        return;
    };

    if phone.visible_category_count > MAX_CATEGORY_CONTROLS {
        problems.push(format!(
            "{} primary category controls are visible; #199 allows at most \
             {MAX_CATEGORY_CONTROLS}",
            phone.visible_category_count
        ));
    }
    if phone.visible_category_count != EXPECTED_CATEGORIES.len() {
        problems.push(format!(
            "expected exactly {} category controls (the seven current actions span exactly \
             that many categories), the snapshot reports {} -- either a category failed to \
             render or `combat::actions` changed without updating this scenario's \
             EXPECTED_CATEGORIES",
            EXPECTED_CATEGORIES.len(),
            phone.visible_category_count
        ));
    }

    match expected_open {
        None => {
            if palette.button_count != 0 || !phone.open_action_ids.is_empty() {
                problems.push(format!(
                    "the closed state must show no action buttons, but {} are visible ({:?})",
                    palette.button_count, phone.open_action_ids
                ));
            }
        }
        Some((category, expected_actions)) => {
            if phone.open_category.as_deref() != Some(category) {
                problems.push(format!(
                    "expected open_category {category:?}, the snapshot reports {:?}",
                    phone.open_category
                ));
            }
            let expected_ids: Vec<String> =
                expected_actions.iter().map(|id| id.to_string()).collect();
            if phone.open_action_ids != expected_ids {
                problems.push(format!(
                    "category {category:?} must reveal exactly its registered actions \
                     {expected_ids:?}, the snapshot reports {:?}",
                    phone.open_action_ids
                ));
            }
        }
    }

    if !phone.fits_in_window {
        problems.push(
            "at least one visible palette control's rendered box extends outside the real \
             browser window (see `review::publish_palette_state`)"
                .to_string(),
        );
    }
    if phone.min_target_size < MIN_TOUCH_TARGET {
        problems.push(format!(
            "smallest visible touch target measures {:.1} CSS px; #199 requires at least \
             {MIN_TOUCH_TARGET} on every phone target",
            phone.min_target_size
        ));
    }
    if phone.overlaps_status_panels {
        problems.push(
            "the palette covers a fighter status panel (nameplate/HP/stamina) -- #199 \
             requires the disclosure to reveal actions without covering required fighter/\
             status information (see `review::publish_palette_state`)"
                .to_string(),
        );
    }
    if phone.overlaps_fighter_region {
        problems.push(
            "the palette covers a fighter's readable body area (see \
             `review::fighter_readable_rect`) -- #276 requires the closed palette and every \
             open-category state to leave both fighters readable"
                .to_string(),
        );
    }
    if phone.overlaps_log_panel {
        problems.push(
            "the palette covers the combat log (`hud::LogPanelRoot`) -- #276 requires the \
             closed palette and every open-category state to leave the combat log readable"
                .to_string(),
        );
    }
}
