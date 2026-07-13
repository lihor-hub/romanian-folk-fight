//! The `cold-menu` scenario (#168): builds+serves the WASM game, drives a
//! freshly-launched, cold-cache Chrome at two viewport sizes, and verifies
//! the first painted main menu.
//!
//! ## Readiness contract
//!
//! No time-only sleeps. Each checkpoint polls a real, JS-observable
//! condition once per *rendered frame* (`Checkpoint::wait_for_frame` awaits
//! an actual `requestAnimationFrame`, not a wall-clock timer):
//!
//! 1. **Booted**: `#loading` (see `index.html`) has removed itself --
//!    Trunk's `TrunkApplicationStarted` fired, meaning the wasm module
//!    instantiated and started running -- and `#game-canvas` has a nonzero
//!    backing size.
//! 2. **Assets fetched**: every [`REQUIRED_ASSETS`] path has appeared in
//!    the page's resource-timing entries. Without this, a CPU-saturated CI
//!    runner can starve the wasm app's asset `fetch()` dispatch for 10+
//!    seconds while the canvas keeps re-painting the byte-identical
//!    `GameState::Loading` clear color -- which would satisfy the stability
//!    streak below long before the menu ever had a chance to exist (see
//!    [`required_assets_fetched`]).
//! 3. **Stabilized**: once booted with assets fetched, capture a screenshot
//!    every frame and require [`STABLE_FRAMES_REQUIRED`] byte-identical
//!    captures in a row. A screen that's still transitioning (e.g. the
//!    loading gate hasn't actually finished spawning the menu's UI tree yet
//!    even though the wasm app "started") keeps producing different frames
//!    and never hits the streak; a genuinely painted, static menu does
//!    within a handful of frames.
//!
//! Both conditions are bounded by [`READY_MAX_FRAMES`]/[`READY_MAX_WALL_CLOCK`]
//! -- exceeding the budget is a real, diagnosable checkpoint failure (with
//! full artifacts still captured), never a silent pass.
//!
//! ## Why no `document.fonts.check`/glyph-shape inspection
//!
//! The menu is entirely canvas/WebGL-rendered (`bevy_ui`); there is no DOM
//! text and no CSS `@font-face`, so `document.fonts` knows nothing about
//! the bundled Alegreya font Bevy loads internally. Proving "Alegreya is
//! active" and "diacritics aren't tofu" instead leans on the app's own
//! contract, established by `src/core/mod.rs`'s `GameState::Loading` gate
//! (#114): the menu's UI tree is only ever spawned in `GameState::MainMenu`,
//! which `transition_out_of_loading` only enters once *both*
//! `UiFont`'s and `PanelTexture`'s asset handles report
//! `is_loaded_with_dependencies`. So if this scenario observes a booted,
//! stabilized, non-blank first paint at all, the font and panel texture
//! *must* have finished loading by construction -- reinforced here by also
//! requiring the browser to have actually fetched both asset paths
//! successfully ([`REQUIRED_ASSETS`]). Every text element in the menu goes
//! through the one bundled `UiFont` handle (`src/theme/mod.rs`; there is no
//! fallback font chain), and a unit test elsewhere in the workspace
//! (`core::tests::bundled_font_covers_romanian_diacritics`) already pins
//! that this exact font file maps the required comma-below glyphs -- so
//! there is no code path in which the diacritics render as tofu while the
//! rest of the string renders correctly. This harness does not re-derive
//! that glyph-coverage guarantee via OCR/pixel-shape inspection; see the PR
//! description's "known limitations" for what this does and doesn't catch.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "cold-menu";

struct CheckpointSpec {
    name: &'static str,
    width: u32,
    height: u32,
}

/// Both checkpoints of the one `cold-menu` scenario (#168): desktop and
/// phone, both at DPR 1 (forced in `browser::launch`).
const CHECKPOINTS: &[CheckpointSpec] = &[
    CheckpointSpec {
        name: "desktop",
        width: 1280,
        height: 800,
    },
    CheckpointSpec {
        name: "phone",
        width: 390,
        height: 844,
    },
];

/// `(fetch path suffix, repo-relative source file)` -- the two assets the
/// #114 loading gate blocks the menu on (`src/core/mod.rs`'s `UI_FONT_PATH`,
/// `src/theme/mod.rs`'s `PANEL_BORDER_PATH`), served by `trunk`'s
/// `copy-dir` under `dist/assets/...`.
const REQUIRED_ASSETS: &[(&str, &str)] = &[
    (
        "assets/fonts/Alegreya-Variable.ttf",
        "assets/fonts/Alegreya-Variable.ttf",
    ),
    ("assets/ui/panel_border.png", "assets/ui/panel_border.png"),
];

/// The Romanian menu copy this checkpoint's rendering depends on
/// (`src/menu/mod.rs`) -- referenced here only in documentation/diagnostics;
/// see the module docs for why this harness doesn't OCR them off the
/// screenshot.
#[allow(dead_code)]
const MENU_COPY: &[&str] = &["Luptă nouă", "Continuă", "Setări"];

const READY_MAX_FRAMES: usize = 1800;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);
const STABLE_FRAMES_REQUIRED: usize = 3;

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = crate::web_smoke::build_release("web-smoke: trunk build --release")?;

    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            "web-smoke: serve dist/",
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!("cold-menu: serving dist/ at {}", server.base_url());

    let mut missing_baseline = false;
    for spec in CHECKPOINTS {
        run_checkpoint(
            spec,
            &server,
            update_baselines,
            strict_visual,
            &mut missing_baseline,
        )?;
    }

    if update_baselines {
        println!(
            "\ncold-menu: baselines updated at tests/visual/baselines/{SCENARIO}/ for {} checkpoint(s).",
            CHECKPOINTS.len()
        );
    } else if missing_baseline {
        println!(
            "\ncold-menu: no accepted baseline existed yet for one or more checkpoints -- \
             the non-screenshot assertions above still ran and passed. Re-run with \
             --update-baselines once you've reviewed the captured screenshot(s) to accept them."
        );
    } else {
        println!("\ncold-menu: all checkpoints passed against their accepted baselines.");
    }
    Ok(())
}

struct Readiness {
    booted: bool,
    stabilized: bool,
    frames_observed: usize,
    elapsed: Duration,
}

/// Runs the readiness contract described in the module docs, then runs
/// every first-paint assertion, writing artifacts (screenshot/console/
/// network/viewport/server logs) unconditionally -- on both a pass and a
/// failure -- before returning.
fn run_checkpoint(
    spec: &CheckpointSpec,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let dir = artifacts::checkpoint_dir(SCENARIO, spec.name).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {}: artifacts dir", spec.name),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    // A brand new, empty profile directory per checkpoint -- never reused
    // across checkpoints or runs -- so every capture is a genuinely cold
    // first load (see `browser` module docs).
    let profile_dir = dir.join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let outcome = (|| -> Result<(PageStatus, Vec<u8>, Readiness), String> {
        // DPR 1 always -- `cold-menu` is unchanged by #198's DPR matrix,
        // which extends `gold-journey` instead (see that module's docs).
        let checkpoint = browser::launch(spec.width, spec.height, 1.0, &profile_dir)?;
        let url = format!("{}/", server.base_url());
        checkpoint.navigate(&url)?;
        wait_for_readiness(&checkpoint, spec)
    })();

    let (status, screenshot, readiness) = match outcome {
        Ok(triple) => triple,
        Err(message) => {
            // Nothing to show for a first paint at all (Chrome/navigation
            // itself failed) -- still retain whatever the static server saw.
            let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
            return Err(SmokeError::scenario(
                format!("web-smoke cold-menu[{}]", spec.name),
                message,
                dir,
            ));
        }
    };

    write_artifacts(&dir, spec, &status, &screenshot, server);

    let mut problems = Vec::new();
    if !readiness.booted {
        problems.push(format!(
            "the loading screen never disappeared within {:?}/{} frames (wasm app never booted)",
            READY_MAX_WALL_CLOCK, READY_MAX_FRAMES
        ));
    } else if !readiness.stabilized {
        problems.push(format!(
            "first paint never stabilized within {:?}/{} frames ({} observed)",
            READY_MAX_WALL_CLOCK, READY_MAX_FRAMES, readiness.frames_observed
        ));
        // The streak is gated on the required assets having been fetched
        // (see `required_assets_fetched`), so "never stabilized" is often
        // really "the loading-gate assets never arrived" -- run the asset
        // diagnosis too so the failure names the missing fetch directly.
        check_required_assets(&status, &mut problems);
    } else {
        check_no_console_or_page_errors(&status, &mut problems);
        check_required_assets(&status, &mut problems);
        check_no_unexpected_scroll(spec, &status, &mut problems);
        check_screenshot_pixels(spec, &screenshot, &mut problems);
    }

    if !problems.is_empty() {
        let message = format!(
            "cold-menu[{}] ({}x{}, ready in {:?}, {} frame(s)) failed:\n  - {}",
            spec.name,
            spec.width,
            spec.height,
            readiness.elapsed,
            readiness.frames_observed,
            problems.join("\n  - ")
        );
        return Err(SmokeError::scenario(
            format!("web-smoke cold-menu[{}]", spec.name),
            message,
            dir,
        ));
    }

    match baseline::handle(SCENARIO, spec.name, &screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => {
            println!(
                "cold-menu[{}]: OK ({}x{}) -- baseline updated at {} -- artifacts: {}",
                spec.name,
                spec.width,
                spec.height,
                baseline::baseline_path(SCENARIO, spec.name).display(),
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            println!(
                "cold-menu[{}]: OK ({}x{}) -- no baseline exists yet -- artifacts: {}",
                spec.name,
                spec.width,
                spec.height,
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Matches) => {
            println!(
                "cold-menu[{}]: OK ({}x{}) -- matches accepted baseline -- artifacts: {}",
                spec.name,
                spec.width,
                spec.height,
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Differs {
            diff_pixels,
            total_pixels,
        }) => {
            let diff_paths = baseline::write_diff_triplet(SCENARIO, spec.name, &screenshot, &dir);
            if strict_visual {
                let mut message = format!(
                    "cold-menu[{}] ({}x{}) failed:\n  - screenshot differs from accepted baseline \
                     ({diff_pixels}/{total_pixels} px) under --strict-visual",
                    spec.name, spec.width, spec.height
                );
                if let Ok(paths) = &diff_paths {
                    message.push_str(&format!("\n  diff triplet: {}", paths.describe()));
                }
                return Err(SmokeError::scenario(
                    format!("web-smoke cold-menu[{}]", spec.name),
                    message,
                    dir,
                ));
            }
            println!(
                "cold-menu[{}]: OK ({}x{}) -- differs from accepted baseline ({diff_pixels}/{total_pixels} px; \
                 not a scenario failure by itself unless --strict-visual, see baseline.rs docs) -- artifacts: {}",
                spec.name,
                spec.width,
                spec.height,
                dir.display()
            );
        }
        Err(e) => {
            println!(
                "cold-menu[{}]: WARNING -- baseline comparison failed to run: {e}",
                spec.name
            );
        }
    }

    Ok(())
}

fn write_artifacts(
    dir: &Path,
    spec: &CheckpointSpec,
    status: &PageStatus,
    screenshot: &[u8],
    server: &StaticServer,
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
            "requested: {}x{}\nmeasured inner: {}x{}\nmeasured client: {}x{}\nscroll: {}x{}\ndevicePixelRatio: {}\ncanvas backing size: {}x{}\nerrors: {:?}\n",
            spec.width,
            spec.height,
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
    let _ = artifacts::write_artifact(dir, "server.log", server.request_log().join("\n"));
}

fn wait_for_readiness(
    checkpoint: &Checkpoint,
    spec: &CheckpointSpec,
) -> Result<(PageStatus, Vec<u8>, Readiness), String> {
    let start = Instant::now();
    let mut last_status: Option<PageStatus> = None;
    let mut last_screenshot: Option<Vec<u8>> = None;
    let mut stable_count = 0usize;
    let mut frames_observed = 0usize;

    for _ in 0..READY_MAX_FRAMES {
        if start.elapsed() > READY_MAX_WALL_CLOCK {
            break;
        }
        checkpoint.wait_for_frame()?;
        frames_observed += 1;
        let status = checkpoint.read_status()?;

        // The stability streak may only start once the app is booted AND the
        // #114 loading-gate assets have actually been fetched -- see
        // `required_assets_fetched` for the CI starvation failure mode this
        // guards against (a stuck-in-`Loading` clear color is perfectly
        // stable, but it is not the first paint this scenario is after).
        if !status.app_booted() || !required_assets_fetched(&status) {
            stable_count = 0;
            last_screenshot = None;
            last_status = Some(status);
            continue;
        }

        let screenshot = checkpoint.screenshot_png(spec.width, spec.height)?;
        if last_screenshot.as_deref() == Some(screenshot.as_slice()) {
            stable_count += 1;
        } else {
            stable_count = 1;
        }
        let booted_status = status;
        last_screenshot = Some(screenshot);
        last_status = Some(booted_status);

        if stable_count >= STABLE_FRAMES_REQUIRED {
            return Ok((
                last_status.expect("just set"),
                last_screenshot.expect("just set"),
                Readiness {
                    booted: true,
                    stabilized: true,
                    frames_observed,
                    elapsed: start.elapsed(),
                },
            ));
        }
    }

    let booted = last_status.as_ref().is_some_and(PageStatus::app_booted);
    let status = match last_status {
        Some(status) => status,
        None => checkpoint.read_status()?,
    };
    let screenshot = match last_screenshot {
        Some(shot) => shot,
        None => checkpoint
            .screenshot_png(spec.width, spec.height)
            .unwrap_or_default(),
    };
    Ok((
        status,
        screenshot,
        Readiness {
            booted,
            stabilized: false,
            frames_observed,
            elapsed: start.elapsed(),
        },
    ))
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

/// Whether every [`REQUIRED_ASSETS`] path has appeared in the page's
/// resource-timing entries at all (any status; the post-ready
/// [`check_required_assets`] still validates status/size). Part of the
/// readiness gate, not just the assertions: on a CPU-saturated runner
/// (SwiftShader software rendering on a busy CI host) the wasm app's asset
/// load tasks can take 10+ seconds to even dispatch their `fetch()`es while
/// the canvas keeps re-rendering the identical `GameState::Loading` clear
/// color -- byte-identical frames that would otherwise satisfy the
/// stability streak long before the app ever had a chance to paint the
/// menu. Observed exactly so on CI runs 29083147501/29085154712 (and on an
/// unrelated branch, 29083778372): "required asset never fetched:
/// assets/fonts/Alegreya-Variable.ttf" plus a flat-clear-color screenshot,
/// with the server receiving the font request moments *after* the
/// checkpoint had already given up. Requiring the fetches before the streak
/// may start keeps the readiness loop polling (bounded by the existing
/// [`READY_MAX_WALL_CLOCK`]/[`READY_MAX_FRAMES`] caps) instead of
/// stabilizing on the not-yet-loaded screen.
fn required_assets_fetched(status: &PageStatus) -> bool {
    REQUIRED_ASSETS
        .iter()
        .all(|(suffix, _)| status.resources.iter().any(|r| r.url.ends_with(suffix)))
}

fn check_required_assets(status: &PageStatus, problems: &mut Vec<String>) {
    for (suffix, _source) in REQUIRED_ASSETS {
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

fn check_no_unexpected_scroll(
    spec: &CheckpointSpec,
    status: &PageStatus,
    problems: &mut Vec<String>,
) {
    const EPSILON: f64 = 1.0;
    // The device-metrics override (`browser::launch`) must actually have
    // taken: `--window-size` alone quietly yields a different viewport
    // (observed: 500x705 for a requested 390x844 on macOS headless), which
    // would make every screenshot a crop/pad of the wrong layout.
    if (status.inner_width - f64::from(spec.width)).abs() > EPSILON
        || (status.inner_height - f64::from(spec.height)).abs() > EPSILON
    {
        problems.push(format!(
            "viewport is {}x{}, expected exactly {}x{} (device-metrics override did not take)",
            status.inner_width, status.inner_height, spec.width, spec.height
        ));
    }
    if status.scroll_width > status.client_width + EPSILON {
        problems.push(format!(
            "document scrolls horizontally: scrollWidth {} > clientWidth {} (requested {})",
            status.scroll_width, status.client_width, spec.width
        ));
    }
    if status.scroll_height > status.client_height + EPSILON {
        problems.push(format!(
            "document scrolls vertically: scrollHeight {} > clientHeight {} (requested {})",
            status.scroll_height, status.client_height, spec.height
        ));
    }
    if (status.device_pixel_ratio - 1.0).abs() > f64::EPSILON {
        problems.push(format!(
            "devicePixelRatio was {} (expected 1 -- --force-device-scale-factor=1 should guarantee this)",
            status.device_pixel_ratio
        ));
    }
}

/// Pixel-level proof that *something* painted (not a blank/solid-color
/// canvas) and that the captured image is exactly the requested viewport
/// size at DPR 1. See the module docs for why this doesn't attempt
/// glyph-shape/tofu detection directly.
fn check_screenshot_pixels(
    spec: &CheckpointSpec,
    screenshot_png: &[u8],
    problems: &mut Vec<String>,
) {
    let image = match image::load_from_memory(screenshot_png) {
        Ok(image) => image,
        Err(e) => {
            problems.push(format!("captured screenshot was not a decodable PNG: {e}"));
            return;
        }
    };
    if image.width() != spec.width || image.height() != spec.height {
        problems.push(format!(
            "screenshot was {}x{}, expected {}x{} (DPR 1 at the requested viewport)",
            image.width(),
            image.height(),
            spec.width,
            spec.height
        ));
        return;
    }

    let rgba = image.to_rgba8();
    let mut min = [255u8; 3];
    let mut max = [0u8; 3];
    let mut white_pixels = 0u64;
    let mut total = 0u64;
    for pixel in rgba.pixels() {
        total += 1;
        for channel in 0..3 {
            min[channel] = min[channel].min(pixel[channel]);
            max[channel] = max[channel].max(pixel[channel]);
        }
        if pixel[0] > 250 && pixel[1] > 250 && pixel[2] > 250 {
            white_pixels += 1;
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
             the menu likely never painted (blank canvas)"
        ));
    }
    if total > 0 && white_pixels * 100 / total > 90 {
        problems.push(format!(
            "screenshot is >90% plain white ({white_pixels}/{total} px) -- panel artwork/text likely \
             fell back to an untextured white placeholder"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web_smoke::browser::ResourceEntry;

    /// A `PageStatus` reporting a booted canvas and exactly `resources`.
    fn status_with_resources(resources: Vec<ResourceEntry>) -> PageStatus {
        PageStatus {
            loading_gone: true,
            canvas_present: true,
            canvas_w: 1280.0,
            canvas_h: 800.0,
            inner_width: 1280.0,
            inner_height: 800.0,
            device_pixel_ratio: 1.0,
            scroll_width: 1280.0,
            scroll_height: 800.0,
            client_width: 1280.0,
            client_height: 800.0,
            console: Vec::new(),
            errors: Vec::new(),
            resources,
        }
    }

    fn entry(url: &str) -> ResourceEntry {
        ResourceEntry {
            url: url.to_string(),
            status: 200,
            transfer_size: 1024.0,
        }
    }

    /// The CI starvation regression (runs 29083147501/29085154712, plus
    /// 29083778372 on an unrelated branch): the app is booted and rendering
    /// its flat `Loading` clear color, sibling assets have been fetched, but
    /// the loading-gate font fetch hasn't dispatched yet. Readiness must not
    /// consider that state fetch-complete, or the stability streak starts on
    /// the blank canvas and the checkpoint fails as "never fetched".
    #[test]
    fn required_assets_fetched_is_false_while_the_font_fetch_is_still_pending() {
        let status = status_with_resources(vec![
            entry("http://127.0.0.1:8080/assets/ui/panel_border.png"),
            entry("http://127.0.0.1:8080/assets/backgrounds/village_far.png"),
            entry("http://127.0.0.1:8080/assets/sprites/player.png"),
        ]);
        assert!(
            !required_assets_fetched(&status),
            "a pending Alegreya fetch must keep the readiness loop polling"
        );
    }

    #[test]
    fn required_assets_fetched_requires_every_gate_asset_not_just_one() {
        let status = status_with_resources(vec![entry(
            "http://127.0.0.1:8080/assets/fonts/Alegreya-Variable.ttf",
        )]);
        assert!(
            !required_assets_fetched(&status),
            "the panel texture gates the menu exactly like the font does (#114)"
        );
    }

    #[test]
    fn required_assets_fetched_accepts_both_gate_assets_among_unrelated_entries() {
        let status = status_with_resources(vec![
            entry("http://127.0.0.1:8080/assets/sprites/player.png"),
            entry("http://127.0.0.1:8080/assets/fonts/Alegreya-Variable.ttf"),
            entry("http://127.0.0.1:8080/assets/ui/panel_border.png"),
        ]);
        assert!(required_assets_fetched(&status));
    }

    /// Presence is enough for *readiness* -- a 404 must not stall the loop
    /// for the full wall-clock budget, because `check_required_assets` then
    /// reports the non-success status precisely on the very next phase.
    #[test]
    fn required_assets_fetched_counts_a_failed_fetch_as_fetched() {
        let mut font = entry("http://127.0.0.1:8080/assets/fonts/Alegreya-Variable.ttf");
        font.status = 404;
        font.transfer_size = 0.0;
        let status = status_with_resources(vec![
            font,
            entry("http://127.0.0.1:8080/assets/ui/panel_border.png"),
        ]);
        assert!(required_assets_fetched(&status));

        let mut problems = Vec::new();
        check_required_assets(&status, &mut problems);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("non-success status 404")),
            "the assertion phase still fails the checkpoint precisely: {problems:?}"
        );
    }
}
