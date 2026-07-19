//! The `abandon-forfeit` scenario (#217, a child of #146): proves, in a real
//! browser, that **Abandonează** (the paused-fight overlay's forfeit action)
//! clears the run snapshot from real `localStorage`, returns to the main
//! menu, and leaves **Continuă** unable to resume anything -- not merely
//! that a headless `App` reaches the right `GameState`. Extends #168's
//! harness per the documented extension pattern (see `web_smoke::mod`'s
//! module docs): a new module here plus one match arm in
//! `web_smoke::run_scenario` and one entry in `SCENARIOS`.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test flow --lib` and `cargo test save::journeys --lib` already
//! cover the forfeit logic (reset + `SaveStore::clear` + the `AbandonFight`
//! intent) through Bevy's headless `App`. What only a real browser proves:
//! 1. A snapshot genuinely written to real `localStorage` (`rff_save_v1`) by
//!    the hero-confirmation checkpoint is genuinely gone -- read back as
//!    `null` -- after a real click-equivalent press of **Abandonează**, not
//!    just that an in-memory test double's `store.clear()` ran.
//! 2. The main menu that (re)spawns afterward really does grey out
//!    **Continuă** and really does refuse to navigate when it is pressed --
//!    the same production `SnapshotLoad::NoSave` path a completely fresh
//!    install takes.
//!
//! ## Opening the pause overlay: a real key press, not a review command
//!
//! Escape is a `PauseState` substate toggle with no domain side effect (see
//! `combat::pause::toggle_on_esc`) -- a real CDP `Escape` keypress
//! (`Checkpoint::press_key`) exercises the same input path a player's Esc
//! does, exactly like `keyboard_accessibility`/`fight_palette_accessible` use
//! real key presses for input that isn't itself a navigation checkpoint.
//! Pressing the rendered **Abandonează** button, in contrast, goes through
//! the review seam's `pressButton PauseAbandon` command (#217,
//! `review::parse_button`) -- like every other navigation button this
//! harness presses, so the *production* handler's domain side effects
//! (`progression::reset_run`, `SaveStore::clear`) run before the
//! `AbandonFight` intent it emits, never a raw `NextState` write.
//!
//! ## No screenshot baselines
//!
//! Like `corrupt-save-recovery`/`save-reload`, this scenario's pass/fail gate
//! is exact `localStorage`/screen reads, not a pixel diff. Screenshots are
//! still captured as artifacts for human review.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-abandon-forfeit/`, mirroring every other review-seam
//! scenario, so concurrent scenario runs never clobber each other's build
//! output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "abandon-forfeit";

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 800;

/// `localStorage` key the harness writes pending review commands to.
/// Mirrors `crate::review::REVIEW_COMMAND_KEY`.
const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
/// `localStorage` key the game publishes the current screen's name to.
/// Mirrors `crate::review::REVIEW_SCREEN_KEY`.
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";
/// `src/save/mod.rs`'s `STORAGE_KEY` -- the run snapshot's own `localStorage`
/// key.
const SAVE_STORAGE_KEY: &str = "rff_save_v1";

/// The creation preset this journey selects -- any preset works since the
/// duel is never fought (the run is abandoned mid-fight); `Voinicul` is
/// picked for consistency with the other review-seam scenarios.
const HERO_PRESET: &str = "Voinicul";

const BOOT_MAX_FRAMES: usize = 1800;
const BOOT_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);
const SCREEN_MAX_FRAMES: usize = 1800;
const SCREEN_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;
/// Generous settle window after the real Escape keypress for
/// `OnEnter(PauseState::Paused)`'s overlay (and its **Abandonează** button
/// entity) to actually spawn, before this scenario tries to press it.
const PAUSE_OVERLAY_SETTLE_FRAMES: usize = 30;
/// Bound on frames waited to prove a `pressButton Continue` after the
/// forfeit does *not* navigate anywhere (there is no `MenuAction::Continue`
/// component on the disabled marker for it to find) -- deliberately smaller
/// than the real screen-wait budgets above, since this is asserting a
/// negative (nothing happens) rather than waiting for a real transition.
const NO_OP_SETTLE_FRAMES: usize = 120;

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (its pass/fail gate is exact localStorage/screen reads, see its module docs)."
        );
    }

    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-abandon-forfeit/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-abandon-forfeit/ at {}",
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
                "\n{SCENARIO}: Abandonează forfeits the run -- Main Menu, no run snapshot in \
                 localStorage, Continuă unable to resume -- artifacts: {}",
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
        .arg("dist-abandon-forfeit");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (abandon-forfeit)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-abandon-forfeit"))
}

fn workspace_root() -> PathBuf {
    crate::process::workspace_root()
}

fn run_checks(server: &StaticServer) -> Result<(), String> {
    let dir = artifacts::checkpoint_dir(SCENARIO, "journey")
        .map_err(|e| format!("artifacts dir: {e}"))?;
    let profile_dir = dir.join("chrome-profile");
    let _ = std::fs::remove_dir_all(&profile_dir);

    let checkpoint = browser::launch(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, 1.0, &profile_dir)?;
    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url)?;

    let (status, _shot) = wait_for_screen(&checkpoint, "MainMenu", true)?;
    check_no_console_or_page_errors(&status, "initial load")?;
    if read_local_storage_item(&checkpoint, SAVE_STORAGE_KEY)?.is_some() {
        return Err("a fresh profile must not already carry a run snapshot".to_string());
    }

    // menu -> creation -> fight: the hero-confirmation checkpoint autosaves
    // (there is now something to forfeit).
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
    )?;
    wait_for_screen(&checkpoint, "CharacterCreation", false)?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "selectPreset", "preset": HERO_PRESET}),
    )?;
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
    )?;
    wait_for_screen(&checkpoint, "Fight", false)?;

    if read_local_storage_item(&checkpoint, SAVE_STORAGE_KEY)?.is_none() {
        return Err(
            "hero confirmation must autosave a run snapshot before it can be forfeited".to_string(),
        );
    }

    let fight_shot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
    let _ = artifacts::write_artifact(&dir, "1-fight-before-abandon.png", &fight_shot);

    // A real Escape keypress opens the pause overlay (no domain side effect
    // of its own -- see the module docs); generously wait for the overlay
    // (and its Abandonează button entity) to actually spawn before pressing
    // it through the review seam.
    checkpoint.press_key("Escape")?;
    for _ in 0..PAUSE_OVERLAY_SETTLE_FRAMES {
        checkpoint.wait_for_frame()?;
    }
    let overlay_shot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
    let _ = artifacts::write_artifact(&dir, "2-pause-overlay.png", &overlay_shot);

    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "PauseAbandon"}),
    )?;
    let (status_after, shot_after) = wait_for_screen(&checkpoint, "MainMenu", false)?;
    check_no_console_or_page_errors(&status_after, "after abandon")?;
    let _ = artifacts::write_artifact(&dir, "3-main-menu-after-abandon.png", &shot_after);

    let save_after_abandon = read_local_storage_item(&checkpoint, SAVE_STORAGE_KEY)?;
    if save_after_abandon.is_some() {
        return Err(format!(
            "abandon must forfeit the run -- {SAVE_STORAGE_KEY:?} must read back null, found \
             {save_after_abandon:?}"
        ));
    }

    // Continuă must be unable to resume anything: the disabled marker
    // spawned for `SnapshotLoad::NoSave` carries no `MenuAction::Continue`
    // component at all (see `menu::spawn_main_menu`), so
    // `review::parse_button`'s `pressButton Continue` finds no matching
    // button and the production handler never runs -- proved here by
    // observing the screen stay on MainMenu instead of navigating anywhere.
    send_command(
        &checkpoint,
        serde_json::json!({"cmd": "pressButton", "button": "Continue"}),
    )?;
    for _ in 0..NO_OP_SETTLE_FRAMES {
        checkpoint.wait_for_frame()?;
    }
    let screen_after_continue_press = read_local_storage_item(&checkpoint, REVIEW_SCREEN_KEY)?;
    if screen_after_continue_press.as_deref() != Some("MainMenu") {
        return Err(format!(
            "Continuă must be disabled after a forfeit -- pressing it must not navigate away \
             from MainMenu, but the screen is now {screen_after_continue_press:?}"
        ));
    }
    if read_local_storage_item(&checkpoint, SAVE_STORAGE_KEY)?.is_some() {
        return Err(
            "the forfeited snapshot must not reappear after pressing the disabled Continuă \
             marker"
                .to_string(),
        );
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
        "review command was never consumed by the game within {COMMAND_CONSUMED_MAX_FRAMES} \
         frames: {json}"
    ))
}

fn read_local_storage_item(checkpoint: &Checkpoint, key: &str) -> Result<Option<String>, String> {
    checkpoint.eval_string(&format!("localStorage.getItem({key:?})"))
}

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
        "never observed screen `{expected}` within {max_wall_clock:?}/{max_frames} frames (last \
         seen: {last_screen:?})"
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
