//! The `reduced-motion-fight` scenario (#200, a child of #145): proves the
//! reduced-motion preference actually suppresses presentation motion in a
//! real browser, without touching combat itself. Extends #168's harness per
//! the documented extension pattern (see `web_smoke::mod`'s module docs): a
//! new module here plus one match arm in `web_smoke::run_scenario`, reusing
//! #187's review seam (`src/review/mod.rs`) the exact way `gold_journey`
//! does for seeding a deterministic duel.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test arena::fx --lib` / `cargo test arena::animation --lib` prove
//! the *systems* hold parallax at rest, skip camera displacement, and shrink
//! lunge/footwork to the documented nudge -- all through Bevy's headless
//! `App`. What only a real browser proves:
//!
//! 1. The **pre-wasm loader** actually ships the `prefers-reduced-motion`
//!    CSS media query in the built `dist/index.html` (a static check on the
//!    real Trunk output, not the repo source -- see [`assert_loader_css`]).
//! 2. The **persisted preference** (`rff_settings_v1`, seeded into
//!    `localStorage` *before* the wasm module boots, since
//!    `settings::load_settings` runs at `Startup`) actually reaches
//!    `AccessibilityPreferences` in a real, freshly-initialized app -- not
//!    just a headless test fixture that inserts the resource directly.
//! 3. The motion suppression is visible in the **real render loop**, with
//!    the browser's own `requestAnimationFrame` clock driving `Time`, not a
//!    test harness ticking `Time<Virtual>` by hand.
//!
//! ## Reading exact positions instead of diffing screenshots
//!
//! The issue's suggested approach (diffing consecutive screenshots) can't
//! distinguish "parallax held at rest" from "parallax swayed by a pixel
//! that antialiasing absorbed," and a screenshot alone cannot isolate the
//! idle sprite-bob (a legitimate, motion-preference-independent animation,
//! see `arena::idle_bob`) from an actual parallax/lunge regression. Instead,
//! this scenario adds a small, review-feature-only telemetry publish --
//! [`crate::review`]'s `REVIEW_MOTION_KEY`/`MotionSnapshot` in the *game*
//! crate (mirrored here as plain structs, same duplicated-string-literal
//! convention `REVIEW_COMMAND_KEY`/`REVIEW_SCREEN_KEY` already use) -- so
//! this scenario reads exact fighter/camera/parallax-layer x positions every
//! frame and asserts on them directly:
//!
//! - **Idle** (before any combat action): every parallax layer's current x
//!   must equal its rest `base_x` *exactly*, on every sampled frame, and
//!   both fighters must sit exactly on their anchors.
//! - **In combat** (autoplay driving real strikes): the peak fighter
//!   displacement from its anchor (the lunge/footwork treatment) must never
//!   exceed [`REDUCED_MOTION_CEILING`] -- the issue's documented "≤8px"
//!   ceiling, generously above the game's own `REDUCED_MOTION_DISPLACEMENT`
//!   (6.0 units) -- and at least one nonzero displacement must be observed,
//!   so the check cannot vacuously pass if no attack ever happened. The
//!   camera must stay within the same ceiling of its own rest position the
//!   whole time (screen shake, if the fixed seed ever produces a crit, is
//!   suppressed the same way).
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-reduced-motion-fight/` (mirroring
//! `gold_journey::build_review_release`) -- never `dist/` (the ordinary,
//! `cold-menu`-served build) and never `dist-gold-journey/` (a concurrent
//! `gold-journey` run's build), so the three scenarios' builds never clobber
//! each other.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "reduced-motion-fight";

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 800;

/// `localStorage` key this scenario writes pending review commands to.
/// Mirrors `crate::review::REVIEW_COMMAND_KEY` in the *game* crate.
const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
/// `localStorage` key the game publishes the current screen's name to.
/// Mirrors `crate::review::REVIEW_SCREEN_KEY`.
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
/// `localStorage` key the game publishes a `MotionSnapshot` to every frame
/// the arena is up. Mirrors `crate::review::REVIEW_MOTION_KEY`.
const REVIEW_MOTION_KEY: &str = "rff_review_motion_v1";
/// `localStorage` key the settings blob lives under. Mirrors
/// `crate::settings::SETTINGS_KEY`.
const SETTINGS_KEY: &str = "rff_settings_v1";

/// A `SettingsSave` (v2) blob with `reduced_motion: true`, seeded into
/// `localStorage` *before* the wasm module boots (see
/// `Checkpoint::seed_local_storage_before_load`) so `settings::load_settings`
/// (a `Startup` system) finds it waiting and applies it to
/// `AccessibilityPreferences` before the arena ever spawns. Audio values are
/// the ordinary defaults -- this scenario only cares about the accessibility
/// field.
const SEEDED_SETTINGS_JSON: &str =
    r#"{"version":2,"music":5,"sfx":5,"muted":false,"reduced_motion":true,"high_contrast":false}"#;

/// Fixed combat seed -- kept equal to `gold_journey::GOLD_JOURNEY_SEED` /
/// `src/review/mod.rs`'s `gold_journey_seed::GOLD_JOURNEY_SEED`, which pins
/// (via the pure combat engine/AI, no browser) that this exact seed +
/// Voinicul + the autoplay policy produces several real strike exchanges
/// before a player victory -- plenty of attack events to sample lunge
/// displacement from. Not required to match `gold-journey`'s value (this
/// scenario doesn't need the duel to reach any particular screen), but reuse
/// avoids introducing a second seed this codebase would need to separately
/// reason about.
const REDUCED_MOTION_FIGHT_SEED: u64 = 20;
const REDUCED_MOTION_FIGHT_PRESET: &str = "Voinicul";

/// The issue's documented reduced-motion displacement ceiling ("a short fade
/// or a small ≤8px nudge"), generously above the game's own
/// `arena::animation::REDUCED_MOTION_DISPLACEMENT` (6.0 world units/px) so
/// this assertion is about the *contract*, not pinned to the exact tuning
/// constant.
const REDUCED_MOTION_CEILING: f64 = 8.0;

const BOOT_MAX_FRAMES: usize = 1800;
const BOOT_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);
const SCREEN_MAX_FRAMES: usize = 900;
const SCREEN_MAX_WALL_CLOCK: Duration = Duration::from_secs(60);
/// How many frames of idle time (no combat action yet) to sample for the
/// "parallax/fighters never move" assertion -- generous enough that any
/// reintroduced sway (`DRIFT_FREQUENCY` is slow, but the fight screen is
/// reached several seconds into the page's life, well past a sway
/// zero-crossing) would show up as a non-exact `x != base_x` on some frame.
const IDLE_SAMPLE_FRAMES: usize = 90;
/// How many frames of autoplay-driven combat to sample the peak
/// lunge/footwork/shake displacement over.
const COMBAT_SAMPLE_FRAMES: usize = 600;

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (motion is asserted from exact telemetry, not screenshots)."
        );
    }

    let dist_dir = build_review_release()?;
    assert_loader_css(&dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: loader CSS"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-reduced-motion-fight/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-reduced-motion-fight/ at {}",
        server.base_url()
    );

    let dir = artifacts::checkpoint_dir(SCENARIO, "fight").map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: artifacts dir"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    let profile_dir = dir.join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let outcome = run_checks(&dir, &server, &profile_dir);
    let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));

    match outcome {
        Ok(summary) => {
            println!(
                "\n{SCENARIO}: reduced motion held parallax/fighters at rest while idle and \
                 kept every sampled in-combat displacement within {REDUCED_MOTION_CEILING}px \
                 ({summary}) -- artifacts: {}",
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

fn build_review_release() -> Result<PathBuf, SmokeError> {
    let mut cmd = Command::new("trunk");
    cmd.arg("build")
        .arg("--release")
        .arg("--features")
        .arg("review")
        .arg("--dist")
        .arg("dist-reduced-motion-fight");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (reduced-motion-fight)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-reduced-motion-fight"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml always has a parent workspace root")
        .to_path_buf()
}

/// Static check on the *built* `dist/index.html` (Trunk's actual served
/// artifact, not the repo source) for the `prefers-reduced-motion` media
/// query that disables the loader's sliding-bar `@keyframes` animation (see
/// `index.html`'s `.progress::before` rule). Trunk inlines/minifies the
/// `<style>` block but does not rewrite media-query text, so the literal
/// substring survives into the output.
fn assert_loader_css(dist_dir: &std::path::Path) -> Result<(), String> {
    let index_path = dist_dir.join("index.html");
    let html = std::fs::read_to_string(&index_path)
        .map_err(|e| format!("failed to read {}: {e}", index_path.display()))?;
    if !html.contains("prefers-reduced-motion") {
        return Err(format!(
            "{} does not contain a `prefers-reduced-motion` media query -- \
             the pre-wasm loader animation would ignore the OS preference",
            index_path.display()
        ));
    }
    if !html.contains("animation:none") && !html.contains("animation: none") {
        return Err(format!(
            "{} has a `prefers-reduced-motion` query but it does not appear to \
             disable the loader's `animation` property",
            index_path.display()
        ));
    }
    Ok(())
}

fn run_checks(
    dir: &std::path::Path,
    server: &StaticServer,
    profile_dir: &std::path::Path,
) -> Result<String, String> {
    let checkpoint = browser::launch(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, profile_dir)?;
    // Seed the persisted accessibility preference *before* the wasm module's
    // Startup schedule runs -- see SEEDED_SETTINGS_JSON's docs.
    checkpoint.seed_local_storage_before_load(SETTINGS_KEY, SEEDED_SETTINGS_JSON)?;

    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url)?;

    let (status, shot) = wait_for_screen(&checkpoint, "MainMenu", true)?;
    check_no_console_or_page_errors(&status, "initial load (MainMenu)")?;
    let _ = artifacts::write_artifact(dir, "1-menu.png", &shot);

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "seedCombat", "seed": REDUCED_MOTION_FIGHT_SEED}),
    )?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
    )?;
    wait_for_screen(&checkpoint, "CharacterCreation", false)?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "selectPreset", "preset": REDUCED_MOTION_FIGHT_PRESET}),
    )?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
    )?;
    let (fight_status, fight_shot) = wait_for_screen(&checkpoint, "Fight", false)?;
    check_no_console_or_page_errors(&fight_status, "fight start")?;
    let _ = artifacts::write_artifact(dir, "2-fight-start.png", &fight_shot);

    let idle_report = sample_idle(&checkpoint, IDLE_SAMPLE_FRAMES)?;
    let _ = artifacts::write_artifact(dir, "3-idle-report.log", &idle_report);

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
    )?;
    let combat_report = sample_combat(&checkpoint, COMBAT_SAMPLE_FRAMES)?;
    let _ = artifacts::write_artifact(dir, "4-combat-report.log", &combat_report);

    Ok(format!("{idle_report}; {combat_report}"))
}

fn wait_frame(checkpoint: &Checkpoint) -> Result<(), String> {
    checkpoint.wait_for_frame()
}

/// Waits for the review seam's published screen to equal `expected`, bounded
/// by [`BOOT_MAX_FRAMES`]/[`BOOT_MAX_WALL_CLOCK`] when `require_boot` is set
/// (the initial cold load) or [`SCREEN_MAX_FRAMES`]/[`SCREEN_MAX_WALL_CLOCK`]
/// otherwise (an in-page transition, no asset loading involved). Returns the
/// last observed status/screenshot once reached.
fn wait_for_screen(
    checkpoint: &Checkpoint,
    expected: &str,
    require_boot: bool,
) -> Result<(PageStatus, Vec<u8>), String> {
    let (max_frames, max_wall_clock) = if require_boot {
        (BOOT_MAX_FRAMES, BOOT_MAX_WALL_CLOCK)
    } else {
        (SCREEN_MAX_FRAMES, SCREEN_MAX_WALL_CLOCK)
    };
    let start = Instant::now();
    let mut last_screen: Option<String> = None;
    for _ in 0..max_frames {
        if start.elapsed() > max_wall_clock {
            break;
        }
        wait_frame(checkpoint)?;
        let screen =
            checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_SCREEN_KEY}')"))?;
        last_screen = screen.clone();
        let status = checkpoint.read_status()?;
        if require_boot && !status.app_booted() {
            continue;
        }
        if screen.as_deref() == Some(expected) {
            let shot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
            return Ok((status, shot));
        }
    }
    Err(format!(
        "never observed screen `{expected}` within {max_wall_clock:?}/{max_frames} frames \
         (last seen: {last_screen:?})"
    ))
}

fn send_command(checkpoint: &Checkpoint, payload: serde_json::Value) -> Result<(), String> {
    let json = payload.to_string();
    let js_literal = serde_json::to_string(&json).map_err(|e| e.to_string())?;
    checkpoint.eval_unit(&format!(
        "localStorage.setItem('{REVIEW_COMMAND_KEY}', {js_literal});"
    ))?;
    for _ in 0..300 {
        wait_frame(checkpoint)?;
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

/// One parallax layer's rest and current x, mirroring
/// `crate::review::ParallaxSample` in the game crate (see the module docs
/// for why this is a duplicated plain struct, not shared code).
#[derive(serde::Deserialize, Debug, Clone, Copy)]
struct ParallaxSample {
    base_x: f64,
    x: f64,
}

/// Mirrors `crate::review::MotionSnapshot`.
#[derive(serde::Deserialize, Debug, Clone)]
struct MotionSnapshot {
    player_x: f64,
    player_anchor_x: f64,
    enemy_x: f64,
    enemy_anchor_x: f64,
    camera_x: f64,
    camera_y: f64,
    parallax: Vec<ParallaxSample>,
}

fn read_motion(checkpoint: &Checkpoint) -> Result<Option<MotionSnapshot>, String> {
    let raw = checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_MOTION_KEY}')"))?;
    match raw {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("motion snapshot was not valid JSON ({json}): {e}")),
    }
}

/// Samples [`MotionSnapshot`] for `frames` real rendered frames before any
/// combat action has happened and asserts every parallax layer's x equals
/// its rest `base_x` *exactly*, and both fighters sit exactly on their
/// anchors, on every single sampled frame.
fn sample_idle(checkpoint: &Checkpoint, frames: usize) -> Result<String, String> {
    let mut samples = 0usize;
    for i in 0..frames {
        wait_frame(checkpoint)?;
        let Some(motion) = read_motion(checkpoint)? else {
            continue;
        };
        samples += 1;
        for layer in &motion.parallax {
            if layer.x != layer.base_x {
                return Err(format!(
                    "idle frame {i}: parallax layer drifted under reduced motion \
                     (base_x {}, x {}) -- expected exact equality",
                    layer.base_x, layer.x
                ));
            }
        }
        if motion.player_x != motion.player_anchor_x {
            return Err(format!(
                "idle frame {i}: player fighter is off its anchor with no combat \
                 action taken (anchor {}, x {})",
                motion.player_anchor_x, motion.player_x
            ));
        }
        if motion.enemy_x != motion.enemy_anchor_x {
            return Err(format!(
                "idle frame {i}: enemy fighter is off its anchor with no combat \
                 action taken (anchor {}, x {})",
                motion.enemy_anchor_x, motion.enemy_x
            ));
        }
    }
    if samples == 0 {
        return Err(format!(
            "never observed a motion snapshot under {REVIEW_MOTION_KEY:?} across \
             {frames} idle frames -- review::publish_motion_state may not be wired up"
        ));
    }
    Ok(format!(
        "idle: {samples} frame(s) sampled, parallax/fighters exactly at rest throughout"
    ))
}

/// Samples [`MotionSnapshot`] for up to `frames` real rendered frames while
/// autoplay drives real strikes, tracking the peak fighter/camera
/// displacement from rest, and asserts it never exceeds
/// [`REDUCED_MOTION_CEILING`] while also proving at least one nonzero
/// displacement was actually observed (so the assertion isn't vacuous).
fn sample_combat(checkpoint: &Checkpoint, frames: usize) -> Result<String, String> {
    let mut samples = 0usize;
    let mut peak_fighter_offset = 0.0f64;
    let mut peak_camera_offset = 0.0f64;
    let mut camera_rest: Option<(f64, f64)> = None;

    for i in 0..frames {
        wait_frame(checkpoint)?;
        // Stop early once the duel leaves the fight screen (win/loss) --
        // nothing left to sample.
        let screen =
            checkpoint.eval_string(&format!("localStorage.getItem('{REVIEW_SCREEN_KEY}')"))?;
        if screen.as_deref() != Some("Fight") {
            break;
        }
        let Some(motion) = read_motion(checkpoint)? else {
            continue;
        };
        samples += 1;
        let (rest_x, rest_y) = *camera_rest.get_or_insert((motion.camera_x, motion.camera_y));

        let player_offset = (motion.player_x - motion.player_anchor_x).abs();
        let enemy_offset = (motion.enemy_x - motion.enemy_anchor_x).abs();
        let camera_offset =
            ((motion.camera_x - rest_x).powi(2) + (motion.camera_y - rest_y).powi(2)).sqrt();

        peak_fighter_offset = peak_fighter_offset.max(player_offset).max(enemy_offset);
        peak_camera_offset = peak_camera_offset.max(camera_offset);

        if player_offset > REDUCED_MOTION_CEILING {
            return Err(format!(
                "combat frame {i}: player displacement {player_offset:.2}px exceeds the \
                 reduced-motion ceiling of {REDUCED_MOTION_CEILING}px"
            ));
        }
        if enemy_offset > REDUCED_MOTION_CEILING {
            return Err(format!(
                "combat frame {i}: enemy displacement {enemy_offset:.2}px exceeds the \
                 reduced-motion ceiling of {REDUCED_MOTION_CEILING}px"
            ));
        }
        if camera_offset > REDUCED_MOTION_CEILING {
            return Err(format!(
                "combat frame {i}: camera displacement {camera_offset:.2}px exceeds the \
                 reduced-motion ceiling of {REDUCED_MOTION_CEILING}px (screen shake not \
                 suppressed)"
            ));
        }
    }

    if samples == 0 {
        return Err(format!(
            "never observed a motion snapshot under {REVIEW_MOTION_KEY:?} across up to \
             {frames} combat frames"
        ));
    }
    if peak_fighter_offset == 0.0 {
        return Err(
            "autoplay never produced a single fighter displacement -- the combat \
             displacement assertion is vacuous (no attack event landed); increase \
             COMBAT_SAMPLE_FRAMES or check the review seam's autoplay wiring"
                .to_string(),
        );
    }
    Ok(format!(
        "combat: {samples} frame(s) sampled, peak fighter displacement {peak_fighter_offset:.2}px, \
         peak camera displacement {peak_camera_offset:.2}px (ceiling {REDUCED_MOTION_CEILING}px)"
    ))
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
