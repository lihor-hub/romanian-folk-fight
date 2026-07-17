//! The `accessibility-settings-reload` scenario (#191): opens the settings
//! overlay, flips the reduced-motion and high-contrast toggles by clicking
//! them for real (a genuine CDP mouse click, not a JS-synthesized event),
//! reloads the page, and asserts both preferences survived the reload --
//! plus that the page's viewport metadata and `visualViewport` capability
//! now permit browser zoom (see `index.html` and #115's ViewportInfo
//! contract).
//!
//! ## Locating canvas-rendered buttons
//!
//! The whole UI is `bevy_ui` canvas/WebGL output -- there is no DOM element
//! per button, so [`find_wide_button_centers`] locates them by scanning the
//! captured screenshot for the game's own solid `BUTTON_NORMAL` background
//! color (`src/theme/mod.rs`; approximated here in 8-bit sRGB as
//! [`BUTTON_NORMAL_RGB`]). Every "wide" button in this app (`src/menu`'s
//! main-menu buttons, `src/ui_widgets`'s `wide_button`/`wide_button_labeled`
//! used throughout the settings panel) shares the exact same `260x56`
//! logical-pixel size and starting background color, so a band of rows
//! wide/tall enough to match is, by construction, one of those buttons and
//! not the small `48x48` stepper buttons (`Muzică`/`Efecte` `-`/`+`) or any
//! other themed surface. `Checkpoint::click` then dispatches a real CDP
//! mouse click at that band's center pixel.
//!
//! ## Why localStorage, not a re-opened screenshot, proves persistence
//!
//! Neither toggle changes its resting *color* (only its text label, which
//! this harness cannot OCR off a canvas -- see `cold_menu`'s module docs for
//! the same limitation applied to menu copy). Reading
//! `localStorage.getItem('rff_settings_v1')` directly -- once right after
//! both clicks, once again after a real `Page.reload` -- is what actually
//! answers the acceptance question ("do both preferences persist across
//! reloads"): the first read proves the click -> `AccessibilityPreferences`
//! change -> `SettingsStore` write path fired for real; the second proves
//! the same blob (with both fields still `true`) is what a freshly
//! re-initialized wasm app's `Startup` schedule finds waiting for it. The
//! Rust-level parsing/migration/default-fallback logic itself is already
//! exhaustively covered by `cargo test settings --lib`
//! (`src/settings/mod.rs`); this scenario's job is the one thing a unit test
//! cannot reach -- the real browser storage backend surviving a real
//! navigation reload.
//!
//! ## No baselines
//!
//! This scenario has no screenshot baselines (see `baseline`'s module docs
//! for the policy that *does* apply to `cold-menu`): it captures
//! screenshots purely as artifacts/click-target input, not as a pass/fail
//! visual-regression gate. `--update-baselines` is accepted (so the CLI
//! surface stays uniform across scenarios) but has no effect here.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "accessibility-settings-reload";

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 800;

/// Bounds for the initial (and post-reload) cold boot -- same budget
/// `cold-menu` uses, since a reload re-runs the same asset-loading gate.
const READY_MAX_FRAMES: usize = 1800;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);

/// Bounds for settling after an in-page click (opening the settings
/// overlay): no network/asset loading involved, so a much smaller budget
/// than a cold boot suffices.
const UI_SETTLE_MAX_FRAMES: usize = 300;
const UI_SETTLE_MAX_WALL_CLOCK: Duration = Duration::from_secs(20);

const STABLE_FRAMES_REQUIRED: usize = 3;

/// `src/theme/mod.rs`'s `BUTTON_NORMAL` (`Color::srgb(0.50, 0.09, 0.08)`)
/// approximated as 8-bit sRGB (`round(component * 255)`).
const BUTTON_NORMAL_RGB: [u8; 3] = [128, 23, 20];
/// Per-channel tolerance around [`BUTTON_NORMAL_RGB`] -- generous enough to
/// absorb minor rendering/AA differences while staying well clear of
/// `BUTTON_HOVERED`/`BUTTON_PRESSED`/`BUTTON_DISABLED`, each of which
/// differs from `BUTTON_NORMAL` by at least 35 in some channel.
const BUTTON_COLOR_TOLERANCE: i16 = 24;
/// A row counts toward a button's vertical band once at least this many of
/// its pixels match the button color -- generous relative to the ~260px
/// button width so a text-heavy row still counts (a button's label covers
/// only a minority of any row's pixels).
const MIN_ROW_MATCHES: u32 = 80;
/// Consecutive non-matching rows still tolerated inside one band (glyph
/// ascenders/descenders can locally thin the match count without actually
/// ending the button).
const MAX_ROW_GAP: usize = 6;
/// A `wide_button`/`menu_button` is exactly 56px tall; a small `48x48`
/// stepper button is well below this range and is excluded by requiring the
/// band height to fall in this window.
const MIN_BAND_HEIGHT: u32 = 36;
const MAX_BAND_HEIGHT: u32 = 80;
/// The decisive filter against the 48px-wide stepper buttons: a genuine wide
/// button (`260px`) always has at least one row with an unbroken run of
/// matching pixels comfortably above this, even where its label text
/// interrupts most rows; a stepper button (`48px`) never can.
const MIN_RUN_WIDTH: u32 = 180;

/// `src/settings/mod.rs`'s `SETTINGS_KEY` -- the one `localStorage` key both
/// audio and accessibility preferences share.
const SETTINGS_LOCAL_STORAGE_KEY: &str = "rff_settings_v1";

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (see its module docs)."
        );
    }

    let dist_dir =
        crate::web_smoke::build_release(&format!("web-smoke {SCENARIO}: trunk build --release"))?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!("{SCENARIO}: serving dist/ at {}", server.base_url());

    let dir = artifacts::checkpoint_dir(SCENARIO, "reload").map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: artifacts dir"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    // A brand new, empty profile directory -- never reused across runs --
    // so the first navigation is a genuinely cold first load, same
    // convention as `cold_menu`.
    let profile_dir = dir.join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let outcome = run_checks(&dir, &server, &profile_dir);

    let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));

    match outcome {
        Ok(()) => {
            println!(
                "\n{SCENARIO}: both accessibility toggles persisted across a real reload and page zoom is permitted -- artifacts: {}",
                dir.display()
            );
            Ok(())
        }
        Err(message) => Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}"),
            message,
            dir,
        )),
    }
}

fn run_checks(dir: &Path, server: &StaticServer, profile_dir: &Path) -> Result<(), String> {
    // DPR 1 always -- this scenario is unchanged by #198's DPR matrix,
    // which extends `gold-journey` instead (see that module's docs).
    let checkpoint = browser::launch(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, 1.0, profile_dir)?;
    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url)?;

    const MAIN_MENU_LABEL: &str = "the main menu (\"Luptă nouă\", \"Setări\" -- \"Continuă\" is BUTTON_DISABLED-colored while unsaved)";
    let (status, menu_shot) = wait_until_ready(
        &checkpoint,
        true,
        READY_MAX_FRAMES,
        READY_MAX_WALL_CLOCK,
        |_status, screenshot| expect_wide_buttons(screenshot, 2, MAIN_MENU_LABEL).map(|_| ()),
    )?;
    check_no_console_or_page_errors(&status, "initial load")?;
    let _ = artifacts::write_artifact(dir, "1-menu-before-settings.png", &menu_shot);
    write_viewport_log(dir, "1-viewport-before-settings.log", &status);

    let zoom_before = read_zoom_status(&checkpoint)?;
    assert_zoom_permitted(&zoom_before, "before opening settings")?;
    write_zoom_log(dir, "2-zoom-before-settings.log", &zoom_before);

    let menu_buttons = expect_wide_buttons(&menu_shot, 2, MAIN_MENU_LABEL)?;
    let settings_button = menu_buttons[1]; // bottommost = "Setări" (see module docs)
    checkpoint.click(settings_button.0, settings_button.1)?;

    const SETTINGS_PANEL_LABEL: &str =
        "the settings panel (Sunet, Mișcare redusă, Contrast ridicat, Înapoi)";
    let (settings_status, settings_shot) = wait_until_ready(
        &checkpoint,
        false,
        UI_SETTLE_MAX_FRAMES,
        UI_SETTLE_MAX_WALL_CLOCK,
        |_status, screenshot| expect_wide_buttons(screenshot, 4, SETTINGS_PANEL_LABEL).map(|_| ()),
    )?;
    check_no_console_or_page_errors(&settings_status, "settings overlay opened")?;
    let _ = artifacts::write_artifact(dir, "3-settings-panel-opened.png", &settings_shot);

    let toggle_buttons = expect_wide_buttons(&settings_shot, 4, SETTINGS_PANEL_LABEL)?;
    // Spawn order top-to-bottom (see `src/settings/mod.rs::spawn_overlay`):
    // [0] Sunet (mute), [1] Mișcare redusă, [2] Contrast ridicat, [3] Înapoi.
    let reduced_motion_button = toggle_buttons[1];
    let high_contrast_button = toggle_buttons[2];

    checkpoint.click(reduced_motion_button.0, reduced_motion_button.1)?;
    wait_frames(&checkpoint, 3)?;
    checkpoint.click(high_contrast_button.0, high_contrast_button.1)?;
    wait_frames(&checkpoint, 3)?;

    let stored_before_reload = read_settings_local_storage(&checkpoint)?;
    let _ = artifacts::write_artifact(
        dir,
        "4-local-storage-before-reload.json",
        stored_before_reload.clone().unwrap_or_default(),
    );
    let parsed_before = parse_settings_json(stored_before_reload.as_deref())
        .map_err(|e| format!("before reload: {e}"))?;
    assert_accessibility_persisted(&parsed_before, "before reload")?;

    checkpoint.reload()?;
    // #270's maintainer follow-up: this post-reload main-menu probe is the
    // one this scenario observed flake on ("found 0" -- read before the
    // buttons had spawned/colored). Embedding the same exactly-2-wide-
    // buttons check into `wait_until_ready`'s own `ready` predicate (as the
    // initial load above already does) means a pixel-stable-but-uncolored
    // frame is never mistaken for "ready" here either.
    let (status_after, shot_after) = wait_until_ready(
        &checkpoint,
        true,
        READY_MAX_FRAMES,
        READY_MAX_WALL_CLOCK,
        |_status, screenshot| expect_wide_buttons(screenshot, 2, MAIN_MENU_LABEL).map(|_| ()),
    )?;
    check_no_console_or_page_errors(&status_after, "after reload")?;
    let _ = artifacts::write_artifact(dir, "5-menu-after-reload.png", &shot_after);
    write_viewport_log(dir, "6-viewport-after-reload.log", &status_after);
    // Redundant with the `ready` predicate above (already guaranteed by the
    // time `wait_until_ready` returned `Ok`), but kept as an explicit,
    // separately diagnosed assertion -- the same defense-in-depth shape the
    // initial-load check above uses -- and to log the resulting centers as
    // an artifact-adjacent fact.
    let _ = expect_wide_buttons(&shot_after, 2, MAIN_MENU_LABEL)?;

    let zoom_after = read_zoom_status(&checkpoint)?;
    assert_zoom_permitted(&zoom_after, "after reload")?;
    write_zoom_log(dir, "7-zoom-after-reload.log", &zoom_after);

    let stored_after_reload = read_settings_local_storage(&checkpoint)?;
    let _ = artifacts::write_artifact(
        dir,
        "8-local-storage-after-reload.json",
        stored_after_reload.clone().unwrap_or_default(),
    );
    let parsed_after = parse_settings_json(stored_after_reload.as_deref())
        .map_err(|e| format!("after reload: {e}"))?;
    assert_accessibility_persisted(&parsed_after, "after reload")?;

    Ok(())
}

fn wait_frames(checkpoint: &Checkpoint, count: usize) -> Result<(), String> {
    for _ in 0..count {
        checkpoint.wait_for_frame()?;
    }
    Ok(())
}

/// Waits for a rendered, stable frame that *also* satisfies `ready`: when
/// `require_boot` is set, first waits for the #114 loading gate to clear
/// (same contract as `cold_menu::wait_for_readiness`); either way, then
/// waits for [`STABLE_FRAMES_REQUIRED`] byte-identical screenshots in a row
/// **and** `ready(&status, &screenshot)` returning `Ok(())` on that same
/// stable screenshot -- retrying (not accepting the pixel-stable frame) if
/// `ready` says the specific fact it cares about isn't true yet. Bounded by
/// `max_frames`/`max_wall_clock`; exceeding the budget is a diagnosed
/// failure (folding in whatever `ready` most recently reported), never a
/// silent pass.
///
/// Byte-identical pixels alone is not proof that a screen is actually done:
/// a frame can hold perfectly still for several frames in a row before some
/// later system finishes painting it (`update_button_backgrounds` only
/// repaints a button's `BUTTON_NORMAL` background off a `Changed<Interaction>`
/// query, so a freshly spawned button can sit at whatever placeholder
/// `BackgroundColor` it was spawned with -- itself pixel-stable -- for a
/// frame or more before its real color lands). Accepting pixel stability as
/// a proxy for "the specific fact I need is now true," and asserting that
/// fact only once, afterward, is exactly the shape of the observed CI flake
/// this closes: `accessibility-settings-reload` failed once with "expected
/// exactly 2 wide BUTTON_NORMAL buttons on the main menu ... found 0" -- a
/// stable-looking frame read before the menu's buttons had actually
/// spawned/colored (see #270's maintainer follow-up). Folding the specific
/// expected fact into the readiness gate itself, instead of trusting
/// stability as a stand-in for it, is the same "wait for the specific
/// expected state, not a fixed settle or first-change" principle
/// `keyboard_accessibility`'s navigate-then-activate hardening applies.
fn wait_until_ready(
    checkpoint: &Checkpoint,
    require_boot: bool,
    max_frames: usize,
    max_wall_clock: Duration,
    ready: impl Fn(&PageStatus, &[u8]) -> Result<(), String>,
) -> Result<(PageStatus, Vec<u8>), String> {
    let start = Instant::now();
    let mut last_status: Option<PageStatus> = None;
    let mut last_screenshot: Option<Vec<u8>> = None;
    let mut stable_count = 0usize;
    let mut last_ready_problem: Option<String> = None;

    for _ in 0..max_frames {
        if start.elapsed() > max_wall_clock {
            break;
        }
        checkpoint.wait_for_frame()?;
        let status = checkpoint.read_status()?;

        if require_boot && !status.app_booted() {
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
            let status_ref = last_status.as_ref().expect("just set");
            let screenshot_ref = last_screenshot.as_deref().expect("just set");
            match ready(status_ref, screenshot_ref) {
                Ok(()) => {
                    return Ok((
                        last_status.expect("just set"),
                        last_screenshot.expect("just set"),
                    ));
                }
                Err(problem) => {
                    // Pixel-stable, but not yet the specific state this
                    // caller needs -- keep polling instead of accepting a
                    // false-positive "ready" (see this function's doc
                    // comment).
                    last_ready_problem = Some(problem);
                    stable_count = 0;
                }
            }
        }
    }

    let booted_note = match &last_status {
        Some(status) if require_boot => format!(" (last observed booted={})", status.app_booted()),
        _ => String::new(),
    };
    let ready_note = match last_ready_problem {
        Some(problem) => format!(" (last readiness check: {problem})"),
        None => String::new(),
    };
    Err(format!(
        "screen never reached a stable{} ready state within {max_wall_clock:?}/{max_frames} frames{booted_note}{ready_note}",
        if require_boot { ", booted" } else { "" }
    ))
}

/// Turns a scanned button list into a specific-count-or-diagnosed-error
/// result -- the actual decision [`wait_until_ready`]'s `ready` predicate
/// embeds into the readiness gate itself. Pure given `buttons` (no image
/// decoding here), so the exact success/failure boundary is unit-testable
/// without a screenshot (see the `tests` module below).
fn expect_exact_button_count(
    buttons: &[(f64, f64)],
    expected_count: usize,
    screen_name: &str,
) -> Result<(), String> {
    if buttons.len() == expected_count {
        Ok(())
    } else {
        Err(format!(
            "expected exactly {expected_count} wide BUTTON_NORMAL buttons on {screen_name}, found {}: {buttons:?}",
            buttons.len()
        ))
    }
}

/// Scans `screenshot` for wide `BUTTON_NORMAL` buttons and requires exactly
/// `expected_count` of them ([`expect_exact_button_count`]), returning their
/// centers. Used both as a [`wait_until_ready`] `ready` predicate (so a
/// frame is never accepted as "ready" before the expected buttons have
/// spawned/colored) and, after `wait_until_ready` returns, to get the actual
/// click-target coordinates -- the same scan, not re-derived differently in
/// two places.
fn expect_wide_buttons(
    screenshot: &[u8],
    expected_count: usize,
    screen_name: &str,
) -> Result<Vec<(f64, f64)>, String> {
    let buttons = find_wide_button_centers(screenshot)?;
    expect_exact_button_count(&buttons, expected_count, screen_name)?;
    Ok(buttons)
}

fn check_no_console_or_page_errors(status: &PageStatus, phase: &str) -> Result<(), String> {
    if !status.errors.is_empty() {
        return Err(format!(
            "{phase}: page-level errors observed: {:?}",
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
            "{phase}: console.error observed: {console_errors:?}"
        ));
    }
    Ok(())
}

fn write_viewport_log(dir: &Path, name: &str, status: &PageStatus) {
    let _ = artifacts::write_artifact(
        dir,
        name,
        format!(
            "inner: {}x{}\nclient: {}x{}\nscroll: {}x{}\ndevicePixelRatio: {}\ncanvas backing size: {}x{}\n",
            status.inner_width,
            status.inner_height,
            status.client_width,
            status.client_height,
            status.scroll_width,
            status.scroll_height,
            status.device_pixel_ratio,
            status.canvas_w,
            status.canvas_h,
        ),
    );
}

/// `document.querySelector('meta[name="viewport"]')`'s `content`, plus
/// whether `window.visualViewport` (and its `scale` property) is available
/// -- the two JS-observable signals #191's acceptance criteria ask this
/// scenario to inspect (see `index.html`'s viewport meta and #115's
/// `ViewportInfo` contract).
const ZOOM_STATUS_SCRIPT: &str = r#"
JSON.stringify({
  viewport_meta: (function () {
    var m = document.querySelector('meta[name="viewport"]');
    return m ? m.getAttribute('content') : null;
  })(),
  has_visual_viewport: typeof window.visualViewport !== 'undefined',
  visual_viewport_scale: (window.visualViewport && typeof window.visualViewport.scale === 'number')
    ? window.visualViewport.scale
    : null
})
"#;

#[derive(serde::Deserialize, Debug, Clone)]
struct ZoomStatus {
    viewport_meta: Option<String>,
    has_visual_viewport: bool,
    visual_viewport_scale: Option<f64>,
}

fn read_zoom_status(checkpoint: &Checkpoint) -> Result<ZoomStatus, String> {
    checkpoint.eval_json(ZOOM_STATUS_SCRIPT)
}

/// Fails if the viewport meta still restricts zoom (`maximum-scale`/
/// `user-scalable`, see #191's acceptance criteria) or if
/// `window.visualViewport`/its `scale` is unavailable to confirm the browser
/// can actually report a zoom level back to the page.
fn assert_zoom_permitted(status: &ZoomStatus, phase: &str) -> Result<(), String> {
    let meta = status.viewport_meta.as_deref().unwrap_or("");
    if meta.contains("maximum-scale") || meta.contains("user-scalable") {
        return Err(format!(
            "{phase}: viewport meta still restricts zoom: {meta:?}"
        ));
    }
    if !status.has_visual_viewport {
        return Err(format!(
            "{phase}: window.visualViewport is unavailable; cannot confirm zoom capability"
        ));
    }
    if status.visual_viewport_scale.is_none() {
        return Err(format!(
            "{phase}: window.visualViewport.scale did not report a number"
        ));
    }
    Ok(())
}

fn write_zoom_log(dir: &Path, name: &str, status: &ZoomStatus) {
    let _ = artifacts::write_artifact(
        dir,
        name,
        format!(
            "viewport meta content: {:?}\nhas visualViewport: {}\nvisualViewport.scale: {:?}\n",
            status.viewport_meta, status.has_visual_viewport, status.visual_viewport_scale
        ),
    );
}

/// `{"value": localStorage.getItem(SETTINGS_LOCAL_STORAGE_KEY) }` -- `null`
/// when nothing is stored yet.
fn read_settings_local_storage(checkpoint: &Checkpoint) -> Result<Option<String>, String> {
    #[derive(serde::Deserialize)]
    struct StorageValue {
        value: Option<String>,
    }
    let script = format!(
        "JSON.stringify({{ value: localStorage.getItem({SETTINGS_LOCAL_STORAGE_KEY:?}) }})"
    );
    let result: StorageValue = checkpoint.eval_json(&script)?;
    Ok(result.value)
}

fn parse_settings_json(raw: Option<&str>) -> Result<serde_json::Value, String> {
    let raw = raw.ok_or_else(|| {
        format!("no settings blob stored under localStorage key {SETTINGS_LOCAL_STORAGE_KEY:?}")
    })?;
    serde_json::from_str(raw).map_err(|e| format!("stored settings blob was not valid JSON: {e}"))
}

/// Confirms both accessibility fields round-tripped as `true` -- proving the
/// real click -> `AccessibilityPreferences` -> `SettingsStore` -> browser
/// `localStorage` path fired, not just that *some* blob is present.
fn assert_accessibility_persisted(value: &serde_json::Value, phase: &str) -> Result<(), String> {
    let reduced_motion = value
        .get("reduced_motion")
        .and_then(serde_json::Value::as_bool);
    let high_contrast = value
        .get("high_contrast")
        .and_then(serde_json::Value::as_bool);
    if reduced_motion != Some(true) || high_contrast != Some(true) {
        return Err(format!(
            "{phase}: expected reduced_motion=true and high_contrast=true, found {value}"
        ));
    }
    Ok(())
}

/// One matching row's total match count and its single longest contiguous
/// run of matching pixels, `(run_length, x_start, x_end_inclusive)`.
type RowRun = (u32, u32, u32);

/// Scans a captured screenshot for this app's wide (`260x56`) buttons by
/// their solid `BUTTON_NORMAL` background color (see the module docs for
/// the full rationale) and returns each one's center pixel, sorted
/// top-to-bottom.
fn find_wide_button_centers(screenshot_png: &[u8]) -> Result<Vec<(f64, f64)>, String> {
    let image = image::load_from_memory(screenshot_png)
        .map_err(|e| format!("captured screenshot was not a decodable PNG: {e}"))?;
    if image.width() != VIEWPORT_WIDTH || image.height() != VIEWPORT_HEIGHT {
        return Err(format!(
            "screenshot was {}x{}, expected {}x{}",
            image.width(),
            image.height(),
            VIEWPORT_WIDTH,
            VIEWPORT_HEIGHT
        ));
    }
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();

    let matches_button_color = |x: u32, y: u32| -> bool {
        let pixel = rgba.get_pixel(x, y);
        (0..3).all(|channel| {
            let diff = i16::from(pixel[channel]) - i16::from(BUTTON_NORMAL_RGB[channel]);
            diff.abs() <= BUTTON_COLOR_TOLERANCE
        })
    };

    let mut row_match_count: Vec<u32> = vec![0; height as usize];
    let mut row_best_run: Vec<RowRun> = vec![(0, 0, 0); height as usize];
    for y in 0..height {
        let mut count = 0u32;
        let mut best: RowRun = (0, 0, 0);
        let mut run_start = 0u32;
        let mut run_len = 0u32;
        for x in 0..width {
            if matches_button_color(x, y) {
                count += 1;
                if run_len == 0 {
                    run_start = x;
                }
                run_len += 1;
                if run_len > best.0 {
                    best = (run_len, run_start, x);
                }
            } else {
                run_len = 0;
            }
        }
        row_match_count[y as usize] = count;
        row_best_run[y as usize] = best;
    }

    let mut bands: Vec<(u32, u32)> = Vec::new();
    let mut band_start: Option<u32> = None;
    let mut gap = 0usize;
    let mut last_matching_row = 0u32;
    for y in 0..height {
        if row_match_count[y as usize] >= MIN_ROW_MATCHES {
            if band_start.is_none() {
                band_start = Some(y);
            }
            last_matching_row = y;
            gap = 0;
        } else if band_start.is_some() {
            gap += 1;
            if gap > MAX_ROW_GAP {
                bands.push((band_start.expect("checked is_some"), last_matching_row));
                band_start = None;
                gap = 0;
            }
        }
    }
    if let Some(start) = band_start {
        bands.push((start, last_matching_row));
    }

    let mut centers = Vec::new();
    for (y_start, y_end) in bands {
        let band_height = y_end - y_start + 1;
        if !(MIN_BAND_HEIGHT..=MAX_BAND_HEIGHT).contains(&band_height) {
            continue;
        }
        let widest_run = (y_start..=y_end)
            .map(|y| row_best_run[y as usize])
            .max_by_key(|run| run.0)
            .expect("y_start..=y_end is never empty");
        if widest_run.0 < MIN_RUN_WIDTH {
            continue;
        }
        let center_x = f64::from(widest_run.1 + widest_run.2) / 2.0;
        let center_y = f64::from(y_start + y_end) / 2.0;
        centers.push((center_x, center_y));
    }
    centers.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("no NaNs in pixel coordinates"));
    Ok(centers)
}

#[cfg(test)]
mod tests {
    use super::*;

    // `expect_exact_button_count` -- the pure decision `wait_until_ready`'s
    // `ready` predicate embeds into the readiness gate (#270's maintainer
    // follow-up: fold the specific expected fact into the wait itself,
    // rather than trusting pixel stability as a stand-in for it and
    // asserting the real fact only once, afterward). Red-first: these pin
    // the exact success/failure boundary without needing a screenshot.

    #[test]
    fn expect_exact_button_count_ok_when_the_count_matches() {
        let buttons = vec![(10.0, 20.0), (10.0, 80.0)];
        assert!(expect_exact_button_count(&buttons, 2, "the main menu").is_ok());
    }

    #[test]
    fn expect_exact_button_count_errs_when_none_are_found() {
        // The exact observed CI shape (#270's maintainer follow-up): a
        // stable-looking frame read before any button had spawned/colored.
        let buttons: Vec<(f64, f64)> = vec![];
        let err = expect_exact_button_count(&buttons, 2, "the main menu").unwrap_err();
        assert!(
            err.contains("expected exactly 2")
                && err.contains("the main menu")
                && err.contains("found 0"),
            "error should name the expected count, the screen, and the actual count: {err}"
        );
    }

    #[test]
    fn expect_exact_button_count_errs_when_too_many_are_found() {
        let buttons = vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
        let err = expect_exact_button_count(&buttons, 2, "the settings panel").unwrap_err();
        assert!(err.contains("found 3"));
    }
}
