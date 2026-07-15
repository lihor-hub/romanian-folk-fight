//! The `corrupt-save-recovery` scenario (#201, a child of #146): proves the
//! run-snapshot recovery path in a real browser, for both failure classes
//! `save::storage::SnapshotLoad` distinguishes -- corrupt/unparseable data
//! and a save written by a newer build than this one. Extends #168's harness
//! per the documented extension pattern (see `web_smoke::mod`'s module
//! docs): a new module here plus one match arm in `web_smoke::run_scenario`
//! and one entry in `SCENARIOS`.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test save::storage --lib` and `cargo test menu --lib` already
//! exhaustively cover the classification (`SnapshotLoad::Invalid`/
//! `FutureVersion`) and the menu's resulting UI state/recovery-click
//! behavior through Bevy's headless `App`. What only a real browser proves:
//!
//! 1. A corrupt or future-version blob actually seeded into real
//!    `localStorage` (`rff_save_v1`, `src/save/mod.rs`'s `STORAGE_KEY`) is
//!    read by a freshly-booted wasm app's `Startup`/`OnEnter(MainMenu)` path
//!    and produces the Romanian recovery button on screen, with **no
//!    console error** -- not just that the Rust classification function
//!    returns the right enum variant in a test fixture.
//! 2. Pressing that real, rendered button (driven through #187's review seam
//!    exactly like every other navigation button this harness presses)
//!    actually reaches the browser's `localStorage.removeItem`, observed by
//!    reading `rff_save_v1` back out afterward.
//! 3. The settings blob (`rff_settings_v1`, a completely separate
//!    `localStorage` key/`SettingsStore`) seeded alongside the corrupt save
//!    survives that same recovery click byte-for-byte -- the real-browser
//!    proof of #201's "recovery clears/quarantines only the run snapshot"
//!    acceptance criterion, the browser-level sibling of
//!    `settings::tests::recovering_a_corrupt_run_save_clears_only_the_run_snapshot_never_the_settings`.
//!
//! ## Reading the recovery button's Romanian label, not a pixel color
//!
//! The whole UI is canvas-rendered `bevy_ui` -- there is no DOM element to
//! query. Like `keyboard_accessibility`/`touch_targets`/`zoom_200`, this
//! scenario reads `src/review/mod.rs`'s `AccessibilitySnapshot` (published
//! under `REVIEW_ACCESSIBILITY_KEY` every frame) while cycling focus with
//! real CDP `ArrowRight` presses, asserting the exact rendered label
//! (`RECOVER_SAVE_LABEL`, mirroring `crate::menu::RECOVER_SAVE_LABEL`) shows
//! up in the lap and that no control is labeled **Continuă** (the menu never
//! spawns that label at all while the stored snapshot is
//! invalid/future-version -- see `menu::spawn_main_menu`'s
//! `SnapshotLoad::Invalid | SnapshotLoad::FutureVersion` arm).
//!
//! ## Seeding before the wasm module boots
//!
//! Both `rff_save_v1` and `rff_settings_v1` are seeded via
//! `Checkpoint::seed_local_storage_before_load` *before* `navigate` --
//! `menu::spawn_main_menu` (`OnEnter(GameState::MainMenu)`) and
//! `settings::load_settings` (`Startup`) both run during the very first
//! frames of a cold boot, so seeding after the fact would be too late (the
//! same reasoning `reduced_motion_fight`'s `SEEDED_SETTINGS_JSON` doc
//! comment gives).
//!
//! ## Two independent checkpoints, one per failure class
//!
//! `run_corrupt_checkpoint` and `run_future_version_checkpoint` each launch
//! their own fresh browser/profile (never reused -- see `browser::launch`'s
//! module docs) and drive the identical seed -> boot -> assert -> recover ->
//! assert sequence, differing only in which JSON they seed under
//! `rff_save_v1`. Both are exercised so the evidence bundle covers "corrupt"
//! and "future" separately, per #201's acceptance criteria.
//!
//! ## No screenshot baselines
//!
//! Like `accessibility-settings-reload`/`reduced-motion-fight`/
//! `keyboard-accessibility`, this scenario's pass/fail gate is exact
//! `localStorage`/telemetry reads, not a pixel diff. Screenshots are still
//! captured as artifacts for human review.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-corrupt-save-recovery/`, mirroring every other
//! review-seam scenario, so concurrent scenario runs never clobber each
//! other's build output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "corrupt-save-recovery";

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 800;

/// `localStorage` key the harness writes pending review commands to.
/// Mirrors `crate::review::REVIEW_COMMAND_KEY`.
const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
/// `localStorage` key the game publishes the current screen's name to.
/// Mirrors `crate::review::REVIEW_SCREEN_KEY`.
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
/// `localStorage` key the game publishes an `AccessibilitySnapshot` to every
/// frame. Mirrors `crate::review::REVIEW_ACCESSIBILITY_KEY`.
const REVIEW_ACCESSIBILITY_KEY: &str = "rff_review_a11y_v1";

/// `src/save/mod.rs`'s `STORAGE_KEY` -- the run snapshot's own `localStorage`
/// key, entirely separate from the settings key below.
const SAVE_STORAGE_KEY: &str = "rff_save_v1";
/// `src/settings/mod.rs`'s `SETTINGS_KEY` -- storage the recovery action must
/// never touch (#201's "only the run snapshot" acceptance criterion).
const SETTINGS_STORAGE_KEY: &str = "rff_settings_v1";

/// Deliberately not valid JSON at all -- exercises
/// `save::snapshot::SnapshotLoadError::Invalid` via a failed parse, the same
/// class of corruption a torn/partial write would leave behind.
const CORRUPT_SAVE_JSON: &str = "this is not json, and definitely not a save";

/// A well-formed but unsupported-future version. `SaveGame::load` classifies
/// purely from the payload's `"version"` field (a `VersionProbe` peek, see
/// that module's docs) before ever parsing the rest of the shape, so this
/// minimal object alone is enough to exercise
/// `save::snapshot::SnapshotLoadError::FutureVersion`.
const FUTURE_VERSION_SAVE_JSON: &str = r#"{"version":999}"#;

/// A valid v2 settings blob with distinctive values, seeded alongside the run
/// snapshot in every checkpoint so the scenario can prove recovery leaves it
/// byte-for-byte untouched.
const SETTINGS_SENTINEL_JSON: &str =
    r#"{"version":2,"music":7,"sfx":3,"muted":false,"reduced_motion":true,"high_contrast":false}"#;

/// The exact Romanian recovery button label `menu::spawn_main_menu` renders
/// when the stored run snapshot is present but unusable. Mirrors
/// `crate::menu::RECOVER_SAVE_LABEL`.
const RECOVER_SAVE_LABEL: &str = "Șterge salvarea coruptă";

const BOOT_MAX_FRAMES: usize = 1800;
const BOOT_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);
const SETTLE_MAX_FRAMES: usize = 120;
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;
const CLEAR_MAX_FRAMES: usize = 120;
/// Generous upper bound on ArrowRight presses to discover one full lap --
/// comfortably above the main menu's actual control count (three on the wasm
/// build: **Luptă nouă**, the Continuă/recovery slot, **Setări** -- **Ieși**
/// is native-only).
const MAX_LAP_PRESSES: usize = 20;

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (its pass/fail gate is exact localStorage/telemetry reads, see its module docs)."
        );
    }

    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-corrupt-save-recovery/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-corrupt-save-recovery/ at {}",
        server.base_url()
    );

    let outcome = run_checks(&server);
    let _ = artifacts::write_artifact(
        &artifacts::scenario_dir(SCENARIO),
        "server.log",
        server.request_log().join("\n"),
    );

    match outcome {
        Ok(()) => {
            println!(
                "\n{SCENARIO}: a corrupt and a future-version run snapshot both show the \
                 Romanian recovery action with no console error, and recovery clears only the \
                 run snapshot (settings survive untouched) -- artifacts: {}",
                artifacts::scenario_dir(SCENARIO).display()
            );
            Ok(())
        }
        Err(message) => Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}"),
            message,
            artifacts::scenario_dir(SCENARIO),
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
        .arg("dist-corrupt-save-recovery");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (corrupt-save-recovery)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-corrupt-save-recovery"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml always has a parent workspace root")
        .to_path_buf()
}

fn run_checks(server: &StaticServer) -> Result<(), String> {
    run_recovery_checkpoint(server, "corrupt", CORRUPT_SAVE_JSON)?;
    run_recovery_checkpoint(server, "future-version", FUTURE_VERSION_SAVE_JSON)?;
    Ok(())
}

/// Seeds `seeded_save_json` under [`SAVE_STORAGE_KEY`] (plus
/// [`SETTINGS_SENTINEL_JSON`] under [`SETTINGS_STORAGE_KEY`]) before a fresh
/// cold boot, asserts the Romanian recovery button appears with no console
/// error, presses it through the review seam, and asserts the run snapshot
/// is cleared while the settings blob survives byte-for-byte.
fn run_recovery_checkpoint(
    server: &StaticServer,
    checkpoint_name: &str,
    seeded_save_json: &str,
) -> Result<(), String> {
    let dir = artifacts::checkpoint_dir(SCENARIO, checkpoint_name)
        .map_err(|e| format!("{checkpoint_name}: artifacts dir: {e}"))?;
    let profile_dir = dir.join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let checkpoint = browser::launch(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, 1.0, &profile_dir)?;
    // Seed *before* the wasm module's Startup/OnEnter(MainMenu) systems run
    // -- see the module docs' "seeding before the wasm module boots"
    // section.
    checkpoint.seed_local_storage_before_load(SAVE_STORAGE_KEY, seeded_save_json)?;
    checkpoint.seed_local_storage_before_load(SETTINGS_STORAGE_KEY, SETTINGS_SENTINEL_JSON)?;

    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url)?;

    let (status, shot) = wait_for_screen(&checkpoint, "MainMenu", true)?;
    check_no_console_or_page_errors(&status, &format!("{checkpoint_name}: initial load"))?;
    let _ = artifacts::write_artifact(&dir, "1-recovery-prompt.png", &shot);

    let lap = walk_full_lap(&checkpoint, MAX_LAP_PRESSES)?;
    let labels: Vec<&str> = lap
        .iter()
        .filter_map(|s| s.focused_label.as_deref())
        .collect();
    if !labels.contains(&RECOVER_SAVE_LABEL) {
        return Err(format!(
            "{checkpoint_name}: the Romanian recovery button ({RECOVER_SAVE_LABEL:?}) was never \
             reached by keyboard; saw {labels:?}"
        ));
    }
    if labels.contains(&"Continuă") {
        return Err(format!(
            "{checkpoint_name}: a Continuă-labeled control must not exist while the stored save \
             is invalid/future-version; saw {labels:?}"
        ));
    }
    for required in ["Luptă nouă", "Setări"] {
        if !labels.contains(&required) {
            return Err(format!(
                "{checkpoint_name}: required control {required:?} was never reached; saw {labels:?}"
            ));
        }
    }

    let before_recovery = read_local_storage_item(&checkpoint, SETTINGS_STORAGE_KEY)?;
    if before_recovery.as_deref() != Some(SETTINGS_SENTINEL_JSON) {
        return Err(format!(
            "{checkpoint_name}: the seeded settings blob was not observed intact before \
             recovery (found {before_recovery:?})"
        ));
    }

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "ClearCorruptSave"}),
    )?;

    wait_until_run_save_cleared(&checkpoint, CLEAR_MAX_FRAMES)?;

    let settings_after = read_local_storage_item(&checkpoint, SETTINGS_STORAGE_KEY)?;
    if settings_after.as_deref() != Some(SETTINGS_SENTINEL_JSON) {
        return Err(format!(
            "{checkpoint_name}: recovery must not touch settings storage, but \
             {SETTINGS_STORAGE_KEY:?} changed from the seeded sentinel (now {settings_after:?})"
        ));
    }

    let status_after = checkpoint.read_status()?;
    check_no_console_or_page_errors(&status_after, &format!("{checkpoint_name}: after recovery"))?;
    let final_shot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
    let _ = artifacts::write_artifact(&dir, "2-after-recovery.png", &final_shot);

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
        "review command was never consumed by the game within \
         {COMMAND_CONSUMED_MAX_FRAMES} frames: {json}"
    ))
}

fn read_local_storage_item(checkpoint: &Checkpoint, key: &str) -> Result<Option<String>, String> {
    checkpoint.eval_string(&format!("localStorage.getItem({key:?})"))
}

/// Polls `SAVE_STORAGE_KEY` until it reads back `null` (cleared), bounded by
/// `max_frames` -- the real-browser proof that pressing the recovery button
/// actually reached `localStorage.removeItem`, not just that a review
/// command was accepted.
fn wait_until_run_save_cleared(checkpoint: &Checkpoint, max_frames: usize) -> Result<(), String> {
    for _ in 0..max_frames {
        checkpoint.wait_for_frame()?;
        if read_local_storage_item(checkpoint, SAVE_STORAGE_KEY)?.is_none() {
            return Ok(());
        }
    }
    Err(format!(
        "the run snapshot under {SAVE_STORAGE_KEY:?} was never cleared within {max_frames} \
         frames after pressing the recovery button"
    ))
}

fn wait_for_screen(
    checkpoint: &Checkpoint,
    expected: &str,
    require_boot: bool,
) -> Result<(PageStatus, Vec<u8>), String> {
    let (max_frames, max_wall_clock) = if require_boot {
        (BOOT_MAX_FRAMES, BOOT_MAX_WALL_CLOCK)
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

/// Mirrors `crate::review::AccessibilitySnapshot` -- only the two fields this
/// scenario needs (serde ignores the rest by default).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct AccessibilitySnapshot {
    focused_entity: Option<String>,
    focused_label: Option<String>,
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
/// accessibility snapshot both differs from whatever was published just
/// before the press and names a real focused control -- see
/// `keyboard_accessibility::press_key_and_wait_for_change` for the full
/// rationale (this mirrors it, duplicated per this codebase's per-scenario
/// convention rather than shared).
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

/// Presses `ArrowRight` repeatedly, discovering one full cyclic lap starting
/// from wherever focus currently sits -- see
/// `keyboard_accessibility::walk_full_lap` for the full rationale (mirrored
/// here, duplicated per this codebase's per-scenario convention).
fn walk_full_lap(
    checkpoint: &Checkpoint,
    max_presses: usize,
) -> Result<Vec<AccessibilitySnapshot>, String> {
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
