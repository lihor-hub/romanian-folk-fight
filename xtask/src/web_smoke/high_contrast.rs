//! The `high-contrast` scenario (#214, a child of #145): proves the
//! persisted high-contrast preference actually switches the game's active
//! theme palette in a real browser, at both desktop and phone viewports.
//! Extends #168's harness per the documented extension pattern (see
//! `web_smoke::mod`'s module docs): a new module here plus one match arm in
//! `web_smoke::run_scenario`, reusing #187's review seam
//! (`src/review/mod.rs`) the exact way `gold_journey`/`reduced_motion_fight`
//! already do.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test accessibility_contrast --lib` proves every documented token
//! pair clears its WCAG threshold, and `cargo test combat::hud --lib` proves
//! the HUD spawns/recolors from the active `Palette` resource -- all
//! headlessly. What only a real, freshly-launched browser proves:
//!
//! 1. The **persisted preference** (`rff_settings_v1` with
//!    `high_contrast: true`, seeded into `localStorage` *before* the wasm
//!    module boots, since `settings::load_settings` runs at `Startup`)
//!    actually reaches `AccessibilityPreferences` -> `Palette` in a real,
//!    freshly-initialized app.
//! 2. The switched palette is what the **real render loop** paints with, on
//!    the real menu and a real fight reached through the production path
//!    (menu -> creation -> fight via the review seam's `pressButton`/
//!    `selectPreset` commands).
//!
//! ## Reading the palette from telemetry instead of a screenshot pixel
//!
//! Sampling a single screenshot pixel to detect the palette switch is
//! fragile: font antialiasing, the HP bar's 1.5px gold border, and the
//! 9-slice panel art all blend neighboring colors, so an exact-color
//! pixel probe needs pixel-perfect knowledge of the layout at every
//! viewport. Instead, this scenario reads [`crate::review`]'s
//! `ThemeSnapshot` telemetry (`REVIEW_THEME_KEY`, mirrored here as a plain
//! struct per the established duplicated-string-literal convention): the
//! *exact* sRGB triples of three high-value tokens (`hp_fill`, `bar_track`,
//! `text_primary`) read straight from the live `Palette` resource, asserted
//! against this module's own copies of the expected high-contrast values --
//! duplicated from `src/theme/tokens.rs` deliberately, so a silent retune of
//! the high-contrast palette fails loudly here instead of auto-passing.
//! The captured screenshots are still baselined for human review; the hard
//! pass/fail gate is the exact telemetry fact.
//!
//! ## Grayscale cue-review artifacts (desktop only)
//!
//! After the fight checkpoint, the desktop pass turns on autoplay and saves
//! a short burst of mid-combat screenshots (`combat-sample-*.png`) plus
//! grayscale conversions of every capture (`*-grayscale.png`, ITU-R BT.601
//! luma). These are diagnostics for #214's documented grayscale review --
//! hit/crit/block/miss must stay distinguishable without color, which the
//! text/shape cues (`"6"` / `"12 CRITIC!"` / `"Blocat 3"` / `"Ratat!"`,
//! see `arena::fx`'s module docs) provide -- not baselined and not asserted
//! on (combat timing makes their exact content nondeterministic frame to
//! frame).
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-high-contrast/` directory -- never `dist/` nor any
//! other scenario's dist, so concurrent scenario runs never clobber each
//! other's build output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::desktop_fight_freeze;
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "high-contrast";

/// `localStorage` key this scenario writes pending review commands to.
/// Mirrors `crate::review::REVIEW_COMMAND_KEY` in the *game* crate.
const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
/// `localStorage` key the game publishes the current screen's name to.
/// Mirrors `crate::review::REVIEW_SCREEN_KEY`.
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
/// `localStorage` key the game publishes a `ThemeSnapshot` to every frame.
/// Mirrors `crate::review::REVIEW_THEME_KEY`.
const REVIEW_THEME_KEY: &str = "rff_review_theme_v1";
/// `localStorage` key the settings blob lives under. Mirrors
/// `crate::settings::SETTINGS_KEY`.
const SETTINGS_KEY: &str = "rff_settings_v1";

/// A `SettingsSave` (v2) blob with `high_contrast: true`, seeded into
/// `localStorage` *before* the wasm module boots (see
/// `Checkpoint::seed_local_storage_before_load`) so `settings::load_settings`
/// (a `Startup` system) finds it waiting and `theme::sync_active_palette`
/// switches the `Palette` resource before anything is ever spawned. Audio
/// values are the ordinary defaults; `reduced_motion` stays `false` so this
/// scenario isolates the high-contrast treatment.
const SEEDED_SETTINGS_JSON: &str =
    r#"{"version":2,"music":5,"sfx":5,"muted":false,"reduced_motion":false,"high_contrast":true}"#;

/// The expected high-contrast token colors, as exact `0..=255` sRGB triples.
/// Deliberately duplicated from `src/theme/tokens.rs`'s `HC_*` constants
/// (srgb 0.95/0.20/0.16, 0.05/0.04/0.04, 1.0/1.0/1.0) rather than shared,
/// so a palette retune must consciously update this scenario too -- the same
/// cross-referenced-literal convention every `REVIEW_*_KEY` above uses.
const EXPECTED_HC_HP_FILL: [u8; 3] = [242, 51, 41];
const EXPECTED_HC_BAR_TRACK: [u8; 3] = [13, 10, 10];
const EXPECTED_HC_TEXT_PRIMARY: [u8; 3] = [255, 255, 255];

/// Fixed combat seed -- kept equal to `gold_journey::GOLD_JOURNEY_SEED`
/// (see that constant's docs for why reuse beats a second seed).
const HIGH_CONTRAST_SEED: u64 = 20;
/// `HeroPreset::Voinicul`'s exact display name (see
/// `creation::draft::HeroPreset::name`).
const HIGH_CONTRAST_PRESET: &str = "Voinicul";

/// `(fetch path suffix, repo-relative source file)` -- identical to
/// `cold_menu::REQUIRED_ASSETS`; duplicated rather than shared, per that
/// module's own precedent.
const BASE_REQUIRED_ASSETS: &[(&str, &str)] = &[
    (
        "assets/fonts/Alegreya-Variable.ttf",
        "assets/fonts/Alegreya-Variable.ttf",
    ),
    ("assets/ui/panel_border.png", "assets/ui/panel_border.png"),
];

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

const READY_MAX_FRAMES: usize = 3600;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const STABLE_FRAMES_REQUIRED: usize = 3;
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;
/// How many mid-combat frames to sample for the grayscale cue-review burst
/// (desktop only), and how many frames apart each sample is.
const COMBAT_SAMPLE_COUNT: usize = 6;
const COMBAT_SAMPLE_SPACING_FRAMES: usize = 12;

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-high-contrast/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-high-contrast/ at {}",
        server.base_url()
    );

    let mut missing_baseline = false;
    for viewport in VIEWPORTS {
        run_viewport(
            viewport,
            &server,
            update_baselines,
            strict_visual,
            &mut missing_baseline,
        )?;
    }

    if update_baselines {
        println!(
            "\n{SCENARIO}: baselines updated at tests/visual/baselines/{SCENARIO}/ for {} checkpoint(s).",
            2 * VIEWPORTS.len()
        );
    } else if missing_baseline {
        println!(
            "\n{SCENARIO}: no accepted baseline existed yet for one or more checkpoints -- \
             the non-screenshot assertions above (exact high-contrast palette telemetry, \
             console/asset/scroll checks) still ran and passed. Re-run with \
             --update-baselines once you've reviewed the captured screenshots to accept them."
        );
    } else {
        println!(
            "\n{SCENARIO}: the persisted high-contrast preference switched the palette on \
             menu and fight at both viewports; all checkpoints passed."
        );
    }
    Ok(())
}

fn build_review_release() -> Result<PathBuf, SmokeError> {
    let mut cmd = Command::new("trunk");
    cmd.arg("build")
        .arg("--release")
        .arg("--features")
        .arg("review")
        .arg("--dist")
        .arg("dist-high-contrast");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (high-contrast)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-high-contrast"))
}

fn workspace_root() -> PathBuf {
    crate::process::workspace_root()
}

/// Mirrors `crate::review::ThemeSnapshot`.
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
struct ThemeSnapshot {
    high_contrast: bool,
    hp_fill: [u8; 3],
    bar_track: [u8; 3],
    text_primary: [u8; 3],
}

fn read_theme(checkpoint: &Checkpoint) -> Result<Option<ThemeSnapshot>, String> {
    let raw = checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_THEME_KEY}')"))?;
    match raw {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("theme snapshot was not valid JSON ({json}): {e}")),
    }
}

/// The hard palette-switch gate: the live `Palette` resource must be the
/// exact high-contrast variant, and the preference must have round-tripped.
fn check_theme(snapshot: &ThemeSnapshot, phase: &str) -> Result<(), String> {
    if !snapshot.high_contrast {
        return Err(format!(
            "{phase}: AccessibilityPreferences.high_contrast is false -- the seeded \
             settings blob never reached the preference resource"
        ));
    }
    for (token, actual, expected) in [
        ("hp_fill", snapshot.hp_fill, EXPECTED_HC_HP_FILL),
        ("bar_track", snapshot.bar_track, EXPECTED_HC_BAR_TRACK),
        (
            "text_primary",
            snapshot.text_primary,
            EXPECTED_HC_TEXT_PRIMARY,
        ),
    ] {
        if actual != expected {
            return Err(format!(
                "{phase}: palette token `{token}` is {actual:?}, expected the \
                 high-contrast value {expected:?} -- the Palette resource did not \
                 switch (or src/theme/tokens.rs was retuned without updating this \
                 scenario's EXPECTED_HC_* constants)"
            ));
        }
    }
    Ok(())
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
        "review command was never consumed by the game within {COMMAND_CONSUMED_MAX_FRAMES} frames: {json}"
    ))
}

fn read_screen(checkpoint: &Checkpoint) -> Result<Option<String>, String> {
    checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_SCREEN_KEY}')"))
}

struct Readiness {
    reached_screen: bool,
    stabilized: bool,
    frames_observed: usize,
    elapsed: Duration,
    last_screen: Option<String>,
}

/// One viewport's pass: cold boot to the menu with the preference already
/// seeded, capture + assert; then drive to a fight and capture + assert
/// again; then (desktop only) the autoplay combat burst for the grayscale
/// cue review.
fn run_viewport(
    viewport: &ViewportSpec,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let profile_dir = artifacts::scenario_dir(SCENARIO)
        .join(viewport.name)
        .join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    // DPR 1 always -- this scenario is unchanged by #198's DPR matrix,
    // which extends `gold-journey` instead (see that module's docs).
    let checkpoint =
        browser::launch(viewport.width, viewport.height, 1.0, &profile_dir).map_err(|e| {
            SmokeError::scenario(
                format!("web-smoke {SCENARIO}[{}]", viewport.name),
                e,
                artifacts::scenario_dir(SCENARIO),
            )
        })?;
    // Seed the persisted high-contrast preference *before* the wasm module's
    // Startup schedule runs -- see SEEDED_SETTINGS_JSON's docs.
    step(viewport, || {
        checkpoint.seed_local_storage_before_load(SETTINGS_KEY, SEEDED_SETTINGS_JSON)
    })?;
    let url = format!("{}/", server.base_url());
    step(viewport, || checkpoint.navigate(&url))?;

    // menu: cold boot with the preference already persisted.
    captured_checkpoint(
        &checkpoint,
        viewport,
        "menu",
        "MainMenu",
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    // Drive to a real fight through the production path (see gold_journey's
    // command-by-command rationale).
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "seedCombat", "seed": HIGH_CONTRAST_SEED}),
        )
    })?;
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
        )
    })?;
    wait_for_screen_step(&checkpoint, viewport, "CharacterCreation")?;
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "selectPreset", "preset": HIGH_CONTRAST_PRESET}),
        )
    })?;
    if viewport.name == "desktop" {
        step(viewport, || {
            desktop_fight_freeze::freeze(
                &checkpoint,
                |payload| send_command(&checkpoint, payload),
                || {
                    send_command(
                        &checkpoint,
                        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
                    )
                },
            )
        })?;
    } else {
        step(viewport, || {
            send_command(
                &checkpoint,
                serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
            )
        })?;
    }

    // fight: capture the fresh fight start (full HP/stamina bars in the
    // high-contrast palette).
    captured_checkpoint(
        &checkpoint,
        viewport,
        "fight",
        "Fight",
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    // Desktop only: the grayscale cue-review burst (see the module docs).
    if viewport.name == "desktop" {
        combat_sample_burst(&checkpoint, viewport)?;
    }

    Ok(())
}

/// Runs a fallible step and wraps a failure into a [`SmokeError`] pointing
/// at the scenario's artifact directory.
fn step(
    viewport: &ViewportSpec,
    action: impl FnOnce() -> Result<(), String>,
) -> Result<(), SmokeError> {
    action().map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{}]", viewport.name),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })
}

fn wait_for_screen_step(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    expected: &str,
) -> Result<(), SmokeError> {
    step(viewport, || {
        let start = Instant::now();
        let mut last_screen: Option<String> = None;
        for _ in 0..900 {
            if start.elapsed() > Duration::from_secs(60) {
                break;
            }
            checkpoint.wait_for_frame()?;
            let screen = read_screen(checkpoint)?;
            last_screen = screen.clone();
            if screen.as_deref() == Some(expected) {
                return Ok(());
            }
        }
        Err(format!(
            "never observed screen `{expected}` within 60s/900 frames (last seen: {last_screen:?})"
        ))
    })
}

/// One captured checkpoint: reach the expected screen, freeze
/// `Time<Virtual>` so idle animation can't defeat the byte-identical-frames
/// stability streak (mirrors `gold_journey::captured_checkpoint`), read +
/// assert the theme telemetry, capture + standard assertions + baseline,
/// and unfreeze.
#[allow(clippy::too_many_arguments)]
fn captured_checkpoint(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    name: &str,
    expected_screen: &str,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    if viewport.name != "desktop" || expected_screen != "Fight" {
        wait_for_readiness_screen_only(checkpoint, viewport, expected_screen)?;
        step(viewport, || {
            send_command(
                checkpoint,
                serde_json::json!({"cmd": "setTimePaused", "paused": true}),
            )
        })?;
    }
    let result = capture(
        checkpoint,
        viewport,
        name,
        expected_screen,
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    );
    // Unpause even when the capture failed -- never leave the clock frozen.
    let unpause = step(viewport, || {
        send_command(
            checkpoint,
            serde_json::json!({"cmd": "setTimePaused", "paused": false}),
        )
    });
    result.and(unpause)
}

fn wait_for_readiness_screen_only(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    expected_screen: &str,
) -> Result<(), SmokeError> {
    wait_for_readiness(checkpoint, viewport, expected_screen)
        .map(|_| ())
        .map_err(|e| {
            SmokeError::scenario(
                format!("web-smoke {SCENARIO}[{}][{expected_screen}]", viewport.name),
                e,
                artifacts::scenario_dir(SCENARIO),
            )
        })
}

#[allow(clippy::too_many_arguments)]
fn capture(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    name: &str,
    expected_screen: &str,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let checkpoint_key = format!("{}-{}", viewport.name, name);
    let dir = artifacts::checkpoint_dir(SCENARIO, &checkpoint_key).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{checkpoint_key}]"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    let outcome = wait_for_readiness(checkpoint, viewport, expected_screen);
    let (status, screenshot, readiness) = match outcome {
        Ok(triple) => triple,
        Err(message) => {
            let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
            return Err(SmokeError::scenario(
                format!("web-smoke {SCENARIO}[{checkpoint_key}]"),
                message,
                dir,
            ));
        }
    };

    // The palette-switch telemetry gate, read while the screen is stable.
    let theme = match read_theme(checkpoint) {
        Ok(theme) => theme,
        Err(message) => {
            let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
            return Err(SmokeError::scenario(
                format!("web-smoke {SCENARIO}[{checkpoint_key}]"),
                message,
                dir,
            ));
        }
    };

    write_artifacts(&dir, viewport, &status, &screenshot, server, theme.as_ref());

    let mut problems = Vec::new();
    if !readiness.reached_screen {
        problems.push(format!(
            "never observed screen `{expected_screen}` within {READY_MAX_WALL_CLOCK:?}/{READY_MAX_FRAMES} \
             frames (last seen: {:?})",
            readiness.last_screen
        ));
    } else if !readiness.stabilized {
        problems.push(format!(
            "first paint never stabilized on screen `{expected_screen}` within \
             {READY_MAX_WALL_CLOCK:?}/{READY_MAX_FRAMES} frames ({} observed)",
            readiness.frames_observed
        ));
    } else {
        match theme {
            None => problems.push(format!(
                "never observed a theme snapshot under {REVIEW_THEME_KEY:?} -- \
                 review::publish_theme_state may not be wired up"
            )),
            Some(snapshot) => {
                if let Err(e) = check_theme(&snapshot, &checkpoint_key) {
                    problems.push(e);
                }
            }
        }
        check_no_console_or_page_errors(&status, &mut problems);
        check_required_assets(&status, &mut problems);
        check_no_unexpected_scroll(viewport, &status, &mut problems);
        check_screenshot_pixels(viewport, &screenshot, &mut problems);
    }

    if !problems.is_empty() {
        let message = format!(
            "{SCENARIO}[{checkpoint_key}] ({}x{}, ready in {:?}, {} frame(s)) failed:\n  - {}",
            viewport.width,
            viewport.height,
            readiness.elapsed,
            readiness.frames_observed,
            problems.join("\n  - ")
        );
        return Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{checkpoint_key}]"),
            message,
            dir,
        ));
    }

    match baseline::handle(SCENARIO, &checkpoint_key, &screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => {
            println!(
                "{SCENARIO}[{checkpoint_key}]: OK -- baseline updated -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            println!(
                "{SCENARIO}[{checkpoint_key}]: OK -- no baseline exists yet -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Matches) => {
            println!(
                "{SCENARIO}[{checkpoint_key}]: OK -- matches accepted baseline -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Differs {
            diff_pixels,
            total_pixels,
        }) => {
            let diff_paths =
                baseline::write_diff_triplet(SCENARIO, &checkpoint_key, &screenshot, &dir);
            if strict_visual {
                let mut message = format!(
                    "{SCENARIO}[{checkpoint_key}] failed:\n  - screenshot differs from accepted \
                     baseline ({diff_pixels}/{total_pixels} px) under --strict-visual"
                );
                if let Ok(paths) = &diff_paths {
                    message.push_str(&format!("\n  diff triplet: {}", paths.describe()));
                }
                return Err(SmokeError::scenario(
                    format!("web-smoke {SCENARIO}[{checkpoint_key}]"),
                    message,
                    dir,
                ));
            }
            println!(
                "{SCENARIO}[{checkpoint_key}]: OK -- differs from accepted baseline ({diff_pixels}/{total_pixels} px; \
                 not a scenario failure by itself unless --strict-visual, see baseline.rs docs) -- artifacts: {}",
                dir.display()
            );
        }
        Err(e) => {
            println!(
                "{SCENARIO}[{checkpoint_key}]: WARNING -- baseline comparison failed to run: {e}"
            );
        }
    }

    Ok(())
}

/// Desktop-only diagnostics for the documented grayscale cue review:
/// autoplay drives real strikes and a short burst of mid-combat frames is
/// captured, each alongside a grayscale conversion. Not baselined, not
/// asserted on (see the module docs).
fn combat_sample_burst(checkpoint: &Checkpoint, viewport: &ViewportSpec) -> Result<(), SmokeError> {
    let dir = artifacts::checkpoint_dir(SCENARIO, "desktop-combat-samples").map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[desktop-combat-samples]"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    step(viewport, || {
        send_command(
            checkpoint,
            serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
        )
    })?;
    for sample in 0..COMBAT_SAMPLE_COUNT {
        for _ in 0..COMBAT_SAMPLE_SPACING_FRAMES {
            step(viewport, || checkpoint.wait_for_frame())?;
        }
        // Stop early once the duel resolves -- nothing left to sample.
        let screen = step_value(viewport, || read_screen(checkpoint))?;
        if screen.as_deref() != Some("Fight") {
            break;
        }
        let shot = step_value(viewport, || {
            checkpoint.screenshot_png(viewport.width, viewport.height)
        })?;
        let _ = artifacts::write_artifact(&dir, &format!("combat-sample-{sample}.png"), &shot);
        if let Ok(gray) = grayscale_png(&shot) {
            let _ = artifacts::write_artifact(
                &dir,
                &format!("combat-sample-{sample}-grayscale.png"),
                &gray,
            );
        }
    }
    println!(
        "{SCENARIO}[desktop-combat-samples]: grayscale cue-review burst written -- artifacts: {}",
        dir.display()
    );
    Ok(())
}

/// Like [`step`], for actions that return a value.
fn step_value<T>(
    viewport: &ViewportSpec,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, SmokeError> {
    action().map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{}]", viewport.name),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })
}

/// A PNG re-encoded as 8-bit grayscale (ITU-R BT.601 luma, `image`'s
/// `to_luma8`), for the documented grayscale cue review.
fn grayscale_png(png: &[u8]) -> Result<Vec<u8>, String> {
    let image = image::load_from_memory(png).map_err(|e| e.to_string())?;
    let gray = image.to_luma8();
    let mut out = std::io::Cursor::new(Vec::new());
    gray.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// Waits for `expected_screen` (published by the review seam) and #168's
/// screenshot-stability streak -- exactly `gold_journey::wait_for_readiness`
/// with the spec inlined.
fn wait_for_readiness(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    expected_screen: &str,
) -> Result<(PageStatus, Vec<u8>, Readiness), String> {
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

        let ready_screen = status.app_booted() && screen.as_deref() == Some(expected_screen);
        if !ready_screen {
            stable_count = 0;
            last_screenshot = None;
            last_status = Some(status);
            continue;
        }

        let screenshot = checkpoint.screenshot_png(viewport.width, viewport.height)?;
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

    let reached_screen = last_screen.as_deref() == Some(expected_screen);
    let status = match last_status {
        Some(status) => status,
        None => checkpoint.read_status()?,
    };
    let screenshot = match last_screenshot {
        Some(shot) => shot,
        None => checkpoint
            .screenshot_png(viewport.width, viewport.height)
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
    viewport: &ViewportSpec,
    status: &PageStatus,
    screenshot: &[u8],
    server: &StaticServer,
    theme: Option<&ThemeSnapshot>,
) {
    let _ = artifacts::write_artifact(dir, "screenshot.png", screenshot);
    if let Ok(gray) = grayscale_png(screenshot) {
        let _ = artifacts::write_artifact(dir, "screenshot-grayscale.png", &gray);
    }
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
            viewport.width,
            viewport.height,
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
        "theme.log",
        match theme {
            Some(snapshot) => format!(
                "high_contrast: {}\nhp_fill: {:?} (expected {EXPECTED_HC_HP_FILL:?})\nbar_track: {:?} (expected {EXPECTED_HC_BAR_TRACK:?})\ntext_primary: {:?} (expected {EXPECTED_HC_TEXT_PRIMARY:?})\n",
                snapshot.high_contrast, snapshot.hp_fill, snapshot.bar_track, snapshot.text_primary,
            ),
            None => format!("no theme snapshot observed under {REVIEW_THEME_KEY:?}\n"),
        },
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

fn check_no_unexpected_scroll(
    viewport: &ViewportSpec,
    status: &PageStatus,
    problems: &mut Vec<String>,
) {
    const EPSILON: f64 = 1.0;
    if (status.inner_width - f64::from(viewport.width)).abs() > EPSILON
        || (status.inner_height - f64::from(viewport.height)).abs() > EPSILON
    {
        problems.push(format!(
            "viewport is {}x{}, expected exactly {}x{} (device-metrics override did not take)",
            status.inner_width, status.inner_height, viewport.width, viewport.height
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

fn check_screenshot_pixels(
    viewport: &ViewportSpec,
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
    if image.width() != viewport.width || image.height() != viewport.height {
        problems.push(format!(
            "screenshot was {}x{}, expected {}x{} (DPR 1)",
            image.width(),
            image.height(),
            viewport.width,
            viewport.height,
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
             the screen likely never painted (blank canvas)"
        ));
    }
}
