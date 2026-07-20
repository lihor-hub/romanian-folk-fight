//! The `fight-palette-accessible` scenario (#213, a child of #143): drives a
//! real fight through the #187 review seam (`src/review/mod.rs`) at both a
//! desktop and a phone viewport and proves descriptor-driven keyboard focus
//! end to end — real order, a visible reason on a naturally-disabled
//! control, the visible gold focus marker, and phone category-close
//! recovery — through a *real* browser's keyboard pipeline rather than
//! headless ECS state alone. Extends #168's harness per the documented
//! extension pattern (see `web_smoke::mod`'s module docs): a new module here
//! plus one match arm in `web_smoke::run_scenario`, mirroring
//! `high_contrast`'s two-viewport shape and `fight_palette_phone`'s category
//! interaction.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test combat::action_palette --lib` (the `focus_navigation` module)
//! proves the traversal order, activation, and category-close redirect from
//! directly-injected `ButtonInput`/`Gamepad` component state in a headless
//! `App` — real, but never through an actual keyboard event. `cargo test
//! ui_widgets::focus --lib` proves the shared widget the same way. What only
//! a real, freshly-launched Chrome proves: a real CDP-dispatched keypress
//! (`Checkpoint::press_key`, an `Input.dispatchKeyEvent` pair — not a
//! JS-synthesized event) actually reaches the wasm binary's winit input
//! pipeline and drives [`bevy::input_focus::InputFocus`] the same way a
//! player's physical keyboard would, at real, measured layouts (a real
//! desktop window and a real 390x844 phone canvas).
//!
//! ## `ArrowRight`, not `Tab`
//!
//! [`ui_widgets::focus`](crate)'s registration API documents `Tab`/`Shift+Tab`
//! *or* the arrow keys as equally valid "next"/"previous" input — this
//! scenario drives navigation with `ArrowRight` specifically. Headless
//! Chrome's `Input.dispatchKeyEvent` triggers the *browser's own* native Tab-
//! focus-traversal handling (unlike a JS-synthesized event), and repeated
//! CDP-dispatched `Tab` presses on a canvas-only page were observed to only
//! reliably reach the game's own keyboard handling on the very first press —
//! later presses raced/were consumed by that native handling instead of
//! landing on the wasm binary's input pipeline. `ArrowRight` has no such
//! browser-level default action on this page, so it is the reliable choice
//! for *this* browser-automation harness; `Tab`/`Shift+Tab` are still
//! covered end-to-end by `combat::action_palette`'s own headless
//! `focus_navigation` test module (`desktop_tab_order_matches_the_seven_visible_buttons_left_to_right`
//! et al.), which inject `KeyCode::Tab` directly and are unaffected by a
//! browser's native key handling.
//!
//! ## Reading exact focus facts instead of diffing screenshots
//!
//! Like every review-seam scenario since #189, the hard pass/fail gate is
//! exact telemetry, not a pixel diff: [`crate::review`]'s `PaletteSnapshot`
//! gains an optional `focus` object (#213) exposing which control
//! [`InputFocus`](bevy::input_focus::InputFocus) currently names, whether it
//! is a category button, whether the focused action button is disabled, its
//! exact shown reason/cost text, and whether its `Outline`-based focus
//! marker is actually visible — all read from live components in native
//! Bevy space, the same telemetry-over-pixel-probing reasoning
//! `REVIEW_PALETTE_KEY`'s doc comment already documents. Screenshots are
//! still captured and baselined (`--update-baselines`) for human review, but
//! pixels never gate.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-fight-palette-accessible/` directory (mirroring every
//! other review-seam scenario) so concurrent scenario runs never clobber
//! each other's build output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::desktop_fight_freeze;
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "fight-palette-accessible";

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
/// autoplays or resolves the duel — every assertion happens at a fresh fight
/// start, before either fighter has acted, which is exactly when
/// `step-forward`/`leap-forward` are naturally disabled by the starting
/// distance).
const FIGHT_PALETTE_SEED: u64 = 20;
/// `HeroPreset::Voinicul`'s exact display name (see `creation::draft::HeroPreset::name`).
const FIGHT_PALETTE_PRESET: &str = "Voinicul";

/// The seven current combat actions in `combat::actions::ALL_ACTIONS`'s
/// order — the exact keyboard tab order the desktop bar must produce. A
/// future action added to that array (without also updating this constant)
/// fails loudly here instead of silently under-testing the palette, the same
/// pinning convention `fight_palette_desktop::EXPECTED_BUTTON_COUNT` and
/// `fight_palette_phone::EXPECTED_CATEGORIES` use.
const EXPECTED_DESKTOP_TAB_ORDER: &[&str] = &[
    "quick-strike",
    "heavy-strike",
    "block",
    "rest",
    "step-forward",
    "step-back",
    "leap-forward",
];

/// The naturally-disabled desktop button this scenario checks a reason
/// against: the fight starts at `DuelDistance::starting()` (close range), so
/// `step-forward` is disabled with no stamina/state seeding needed at all.
const DISABLED_ACTION_ID: &str = "step-forward";
const DISABLED_ACTION_REASON: &str = "Ești deja aproape.";

/// The four phone categories in `CATEGORY_ORDER` — the exact keyboard tab
/// order the closed phone bar must produce.
const EXPECTED_PHONE_CATEGORY_ORDER: &[&str] = &["strikes", "defense", "movement", "utility"];
/// The category this scenario opens on phone: its members
/// (`step-forward`/`leap-forward`) are disabled at the fight's starting
/// distance, same as `DISABLED_ACTION_ID` on desktop, and `step-back` is
/// enabled — proving the reason/marker facts render identically once a
/// phone category discloses its actions.
const PHONE_OPEN_CATEGORY: &str = "movement";

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
/// Frame budget for one ArrowRight press (or a `pressActionCategory` command) to be
/// reflected in the next published palette snapshot.
const FOCUS_SETTLE_MAX_FRAMES: usize = 120;
/// Fixed settle window after pressing Enter on a disabled control, which is
/// expected to change nothing -- long enough for a real state change to show
/// up if the "never emits" behavior were broken, short enough not to stall
/// the scenario confirming an intentional no-op.
const ENTER_SETTLE_FRAMES: usize = 30;

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-fight-palette-accessible/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-fight-palette-accessible/ at {}",
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
            "\n{SCENARIO}: baselines updated at tests/visual/baselines/{SCENARIO}/ for {} \
             viewport(s).",
            VIEWPORTS.len()
        );
    } else if missing_baseline {
        println!(
            "\n{SCENARIO}: no accepted baseline existed yet for one or more viewports -- the \
             non-screenshot assertions above (real-keyboard focus order, visible disabled \
             reason, visible focus marker, category-close recovery) still ran and passed. \
             Re-run with --update-baselines once you've reviewed the captured screenshots to \
             accept them."
        );
    } else {
        println!("\n{SCENARIO}: descriptor-driven keyboard focus passed at both viewports.");
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
        .arg("dist-fight-palette-accessible");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (fight-palette-accessible)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-fight-palette-accessible"))
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

/// Mirrors `crate::review::FocusSnapshot` (#213).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct FocusSnapshot {
    focused_id: String,
    focused_is_category: bool,
    focused_is_disabled: bool,
    focused_reason_text: Option<String>,
    focus_marker_visible: bool,
}

/// Mirrors `crate::review::PhonePaletteSnapshot` (only the fields this
/// scenario reads; the deserializer ignores any JSON fields it doesn't name).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct PhonePaletteSnapshot {
    visible_category_count: usize,
    open_category: Option<String>,
}

/// Mirrors `crate::review::PaletteSnapshot`.
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct PaletteSnapshot {
    button_count: usize,
    fits: bool,
    phone: Option<PhonePaletteSnapshot>,
    focus: Option<FocusSnapshot>,
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

/// Presses `key` via a real CDP keypress and waits until the published
/// palette snapshot actually differs from whatever was published just before
/// the press (up to [`FOCUS_SETTLE_MAX_FRAMES`]), returning the changed
/// snapshot. Used for presses that are *expected* to move focus (ArrowRight/
/// ArrowLeft): a real CDP keypress and this harness's own polling are both
/// asynchronous relative to the browser's render loop, so simply reading
/// "whatever snapshot is published next" can race and return a stale,
/// pre-press snapshot -- comparing against the captured "before" value is
/// what makes this robust regardless of exactly how many frames the
/// dispatch takes to land.
fn press_key_and_wait_for_change(
    checkpoint: &Checkpoint,
    key: &str,
) -> Result<PaletteSnapshot, String> {
    let before = read_palette(checkpoint)?;
    checkpoint.press_key(key)?;
    for _ in 0..FOCUS_SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_palette(checkpoint)?
            && Some(&snapshot) != before.as_ref()
        {
            return Ok(snapshot);
        }
    }
    Err(format!(
        "the palette snapshot never changed within {FOCUS_SETTLE_MAX_FRAMES} frames after \
         pressing {key:?} (still {before:?})"
    ))
}

/// Presses `key` via a real CDP keypress, waits a fixed `settle_frames`
/// (draining whatever the harness observes along the way), and returns
/// whatever the palette snapshot reports afterward. Used for a press that is
/// *not* expected to change anything (Enter on a disabled control) --
/// [`press_key_and_wait_for_change`]'s "wait until different" contract would
/// otherwise have to run out its full timeout to confirm the (correct)
/// no-op, which is both slow and indistinguishable from "the harness raced
/// the dispatch"; a short fixed settle is the right tool for asserting
/// stability instead of change.
fn press_key_and_settle(
    checkpoint: &Checkpoint,
    key: &str,
    settle_frames: usize,
) -> Result<PaletteSnapshot, String> {
    checkpoint.press_key(key)?;
    let mut last = None;
    for _ in 0..settle_frames {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_palette(checkpoint)? {
            last = Some(snapshot);
        }
    }
    last.ok_or_else(|| {
        format!(
            "no palette snapshot was published within {settle_frames} frames after pressing {key:?}"
        )
    })
}

struct Readiness {
    reached_screen: bool,
    stabilized: bool,
    frames_observed: usize,
    elapsed: Duration,
    last_screen: Option<String>,
}

/// One viewport's pass: cold boot to the menu, drive to a fresh fight, then
/// exercise real-keyboard focus (order, disabled reason, marker) and, on
/// phone, category-close recovery through `pressActionCategory`.
fn run_viewport(
    viewport: &ViewportSpec,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let checkpoint_name = viewport.name;
    let dir = artifacts::checkpoint_dir(SCENARIO, checkpoint_name).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{checkpoint_name}]"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    let profile_dir =
        artifacts::scenario_dir(SCENARIO).join(format!("chrome-profile-{}", viewport.name));
    let _ = std::fs::remove_dir_all(&profile_dir);

    let outcome = (|| -> Result<(PageStatus, Vec<u8>, Readiness, Vec<String>), String> {
        // DPR 1 (#198's `browser::launch` takes it explicitly now): focus
        // order/marker facts are DPR-independent, and DPR 1 keeps the
        // captured screenshots byte-comparable with this scenario's own
        // baselines; the multi-DPR rendering matrix is `gold-journey`'s job.
        let checkpoint = browser::launch(viewport.width, viewport.height, 1.0, &profile_dir)?;
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
        if viewport.name == "desktop" {
            desktop_fight_freeze::freeze(
                &checkpoint,
                |payload| send_command(&checkpoint, payload),
                || {
                    send_command(
                        &checkpoint,
                        serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
                    )
                },
            )?;
        } else {
            send_command(
                &checkpoint,
                serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
            )?;
            send_command(
                &checkpoint,
                serde_json::json!({"cmd": "setTimePaused", "paused": true}),
            )?;
        }

        let (status, screenshot, readiness) = wait_for_readiness(&checkpoint, viewport)?;
        let mut notes = Vec::new();
        if readiness.reached_screen && readiness.stabilized {
            if viewport.name == "desktop" {
                exercise_desktop_focus(&checkpoint, viewport, &dir, &mut notes)?;
            } else {
                exercise_phone_focus(&checkpoint, viewport, &dir, &mut notes)?;
            }
        }

        let _ = send_command(
            &checkpoint,
            serde_json::json!({"cmd": "setTimePaused", "paused": false}),
        );

        Ok((status, screenshot, readiness, notes))
    })();

    let (status, screenshot, readiness, notes) = match outcome {
        Ok(quad) => quad,
        Err(message) => {
            let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
            return Err(SmokeError::scenario(
                format!("web-smoke {SCENARIO}[{checkpoint_name}]"),
                message,
                dir,
            ));
        }
    };

    write_artifacts(&dir, &status, &screenshot, server, &notes);

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
        check_no_unexpected_scroll(viewport, &status, &mut problems);
        check_screenshot_pixels(viewport, &screenshot, &mut problems);
    }

    if !problems.is_empty() {
        let message = format!(
            "{SCENARIO}[{checkpoint_name}] ({}x{}, ready in {:?}, {} frame(s)) failed:\n  - {}",
            viewport.width,
            viewport.height,
            readiness.elapsed,
            readiness.frames_observed,
            problems.join("\n  - ")
        );
        return Err(SmokeError::scenario(
            format!("web-smoke {SCENARIO}[{checkpoint_name}]"),
            message,
            dir,
        ));
    }

    match baseline::handle(SCENARIO, checkpoint_name, &screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => {
            println!(
                "{SCENARIO}[{checkpoint_name}]: OK -- baseline updated -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            println!(
                "{SCENARIO}[{checkpoint_name}]: OK -- no baseline exists yet -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Matches) => {
            println!(
                "{SCENARIO}[{checkpoint_name}]: OK -- matches accepted baseline -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Differs {
            diff_pixels,
            total_pixels,
        }) => {
            // #198: always write the reviewable actual/expected/diff triplet;
            // fail the checkpoint only under --strict-visual (mirroring
            // `fight_palette_phone`'s handling).
            let diff_paths =
                baseline::write_diff_triplet(SCENARIO, checkpoint_name, &screenshot, &dir);
            if strict_visual {
                let mut message = format!(
                    "{SCENARIO}[{checkpoint_name}] failed:\n  - screenshot differs from accepted \
                     baseline ({diff_pixels}/{total_pixels} px) under --strict-visual"
                );
                if let Ok(paths) = &diff_paths {
                    message.push_str(&format!("\n  diff triplet: {}", paths.describe()));
                }
                return Err(SmokeError::scenario(
                    format!("web-smoke {SCENARIO}[{checkpoint_name}]"),
                    message,
                    dir,
                ));
            }
            println!(
                "{SCENARIO}[{checkpoint_name}]: OK -- differs from accepted baseline \
                 ({diff_pixels}/{total_pixels} px; not a scenario failure by itself unless \
                 --strict-visual, see baseline.rs docs) -- artifacts: {}",
                dir.display()
            );
        }
        Err(e) => {
            println!(
                "{SCENARIO}[{checkpoint_name}]: WARNING -- baseline comparison failed to run: {e}"
            );
        }
    }

    Ok(())
}

/// Desktop focus assertions (#213): real-keyboard ArrowRight order matches
/// [`EXPECTED_DESKTOP_TAB_ORDER`] exactly and wraps; the naturally-disabled
/// [`DISABLED_ACTION_ID`] shows its exact reason with the focus marker
/// visible; pressing Enter on it changes nothing (no emission, no focus
/// jump).
fn exercise_desktop_focus(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    dir: &std::path::Path,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    // #216: the HUD's ⏸ button is its own `TabGroup::new(-1)`, ordered
    // before the palette's group, so the very first ArrowRight press lands
    // there -- a control the palette snapshot's `focus` field deliberately
    // doesn't describe (it is neither an action nor a category button), so
    // a fixed settle (not wait-for-change) hops over it.
    press_key_and_settle(checkpoint, "ArrowRight", ENTER_SETTLE_FRAMES)?;

    let mut seen = Vec::new();
    for _ in 0..EXPECTED_DESKTOP_TAB_ORDER.len() {
        let snapshot = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
        let focus = snapshot.focus.clone().ok_or_else(|| {
            "pressed ArrowRight but the palette snapshot carried no focus facts".to_string()
        })?;
        seen.push(focus.focused_id.clone());
    }
    let expected: Vec<String> = EXPECTED_DESKTOP_TAB_ORDER
        .iter()
        .map(|id| id.to_string())
        .collect();
    if seen != expected {
        return Err(format!(
            "desktop ArrowRight order was {seen:?}, expected {expected:?} (combat::actions::ALL_ACTIONS' \
             order)"
        ));
    }
    notes.push(format!("desktop tab order: {seen:?}"));

    // Two more ArrowRight presses wrap back to the first button: the wrap
    // passes through the HUD's ⏸ stop first (#216, see above).
    press_key_and_settle(checkpoint, "ArrowRight", ENTER_SETTLE_FRAMES)?;
    let wrapped = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
    let wrapped_focus = wrapped.focus.ok_or_else(|| {
        "pressed ArrowRight but the palette snapshot carried no focus facts".to_string()
    })?;
    if wrapped_focus.focused_id != EXPECTED_DESKTOP_TAB_ORDER[0] {
        return Err(format!(
            "a further ArrowRight press should wrap back to {:?} (via the ⏸ stop), focus is \
             now on {:?}",
            EXPECTED_DESKTOP_TAB_ORDER[0], wrapped_focus.focused_id
        ));
    }

    // Focus is on `quick-strike` (index 0) again after the wrap. Advance
    // three more times to `rest` (index 3), then once more to `step-forward`
    // (index 4) to check its disabled reason.
    for _ in 0..3 {
        press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
    }
    let disabled_snapshot = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
    let disabled_focus = disabled_snapshot.focus.ok_or_else(|| {
        "pressed ArrowRight but the palette snapshot carried no focus facts".to_string()
    })?;
    check_disabled_focus(checkpoint, viewport, dir, &disabled_focus, notes)?;

    // Enter on a disabled, focused control must not move focus or spawn a
    // new state (unit tests already prove it never emits a combat command;
    // this is the real-browser plausibility check).
    let after_enter = press_key_and_settle(checkpoint, "Enter", ENTER_SETTLE_FRAMES)?;
    let after_enter_focus = after_enter.focus.ok_or_else(|| {
        "pressed Enter but the palette snapshot carried no focus facts".to_string()
    })?;
    if after_enter_focus.focused_id != DISABLED_ACTION_ID {
        return Err(format!(
            "pressing Enter on the disabled {DISABLED_ACTION_ID:?} button moved focus to {:?} -- \
             a disabled action must be inert",
            after_enter_focus.focused_id
        ));
    }

    Ok(())
}

/// Phone focus assertions (#213): real-keyboard ArrowRight order over the closed
/// category row matches [`EXPECTED_PHONE_CATEGORY_ORDER`]; opening
/// [`PHONE_OPEN_CATEGORY`] (via the same `pressActionCategory` production
/// toggle #199's own scenario uses) and tabbing once more reaches its
/// naturally-disabled `step-forward` action with a visible reason; closing
/// the category again recovers focus onto the category's own button.
fn exercise_phone_focus(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    dir: &std::path::Path,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    // #216: hop over the HUD's ⏸ stop first -- see
    // `exercise_desktop_focus`'s note on the same press.
    press_key_and_settle(checkpoint, "ArrowRight", ENTER_SETTLE_FRAMES)?;

    let mut seen = Vec::new();
    for _ in 0..EXPECTED_PHONE_CATEGORY_ORDER.len() {
        let snapshot = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
        let focus = snapshot.focus.clone().ok_or_else(|| {
            "pressed ArrowRight but the palette snapshot carried no focus facts".to_string()
        })?;
        if !focus.focused_is_category {
            return Err(format!(
                "expected a category button focused while every category is closed, got action \
                 button {:?}",
                focus.focused_id
            ));
        }
        seen.push(focus.focused_id.clone());
    }
    let expected: Vec<String> = EXPECTED_PHONE_CATEGORY_ORDER
        .iter()
        .map(|id| id.to_string())
        .collect();
    if seen != expected {
        return Err(format!(
            "phone closed-state ArrowRight order was {seen:?}, expected {expected:?} \
             (combat::actions::CATEGORY_ORDER)"
        ));
    }
    notes.push(format!("phone closed tab order: {seen:?}"));

    // Focus is now on the last category (Utility). Open Movement via the
    // production `pressActionCategory` toggle (the same one #199's own
    // scenario uses) -- a mouse/touch tap never moves keyboard focus by
    // itself, matching `combat::action_palette`'s
    // `focus_left_on_a_category_button_survives_opening_a_different_category`
    // unit test.
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressActionCategory", "category": PHONE_OPEN_CATEGORY}),
    )?;
    let opened = wait_for_open_category(checkpoint, PHONE_OPEN_CATEGORY)?;
    if opened.focus.as_ref().is_some_and(|f| f.focused_is_category)
        && opened.focus.as_ref().unwrap().focused_id != "utility"
    {
        return Err(format!(
            "opening {PHONE_OPEN_CATEGORY:?} unexpectedly moved focus off the category button \
             it was on before the tap: {:?}",
            opened.focus
        ));
    }

    // Wrapping from Utility (last of the palette's group) passes through the
    // HUD's ⏸ stop (#216) before reaching the newly-open action row's first
    // button, `step-forward` -- disabled at the fight's starting distance.
    press_key_and_settle(checkpoint, "ArrowRight", ENTER_SETTLE_FRAMES)?;
    let disabled_snapshot = press_key_and_wait_for_change(checkpoint, "ArrowRight")?;
    let disabled_focus = disabled_snapshot.focus.ok_or_else(|| {
        "pressed ArrowRight but the palette snapshot carried no focus facts".to_string()
    })?;
    check_disabled_focus(checkpoint, viewport, dir, &disabled_focus, notes)?;

    // Close Movement again (same production toggle) -- category-close
    // recovery must land focus back on Movement's own category button, the
    // still-alive control whose tap made the just-focused action button
    // disappear.
    send_command(
        checkpoint,
        serde_json::json!({"cmd": "pressActionCategory", "category": PHONE_OPEN_CATEGORY}),
    )?;
    let closed = wait_for_closed_category(checkpoint)?;
    let closed_focus = closed.focus.ok_or_else(|| {
        "closed the category but the palette snapshot carried no focus facts".to_string()
    })?;
    if !closed_focus.focused_is_category || closed_focus.focused_id != PHONE_OPEN_CATEGORY {
        return Err(format!(
            "closing {PHONE_OPEN_CATEGORY:?} must move focus to its own category button, got \
             {:?} (is_category={})",
            closed_focus.focused_id, closed_focus.focused_is_category
        ));
    }
    notes.push(format!(
        "category-close recovery: focus landed on {:?}",
        closed_focus.focused_id
    ));

    Ok(())
}

/// Shared assertion for both viewports: the focused control must be a
/// disabled action button showing [`DISABLED_ACTION_REASON`] with the
/// visible focus marker on. Also captures a supplementary (non-baselined)
/// `focused-disabled.png` screenshot for human review -- the PR evidence
/// this issue asks for ("desktop/phone screenshots with a reason visible"),
/// captured with the gold focus marker actually on screen rather than the
/// pristine fight-start capture [`wait_for_readiness`] already takes before
/// any key is pressed.
fn check_disabled_focus(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    dir: &std::path::Path,
    focus: &FocusSnapshot,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if focus.focused_id != DISABLED_ACTION_ID {
        return Err(format!(
            "expected focus on {DISABLED_ACTION_ID:?}, got {:?}",
            focus.focused_id
        ));
    }
    if !focus.focused_is_disabled {
        return Err(format!(
            "{DISABLED_ACTION_ID:?} must be disabled at the fight's starting (close) distance"
        ));
    }
    if focus.focused_reason_text.as_deref() != Some(DISABLED_ACTION_REASON) {
        return Err(format!(
            "expected the visible reason {DISABLED_ACTION_REASON:?} on {DISABLED_ACTION_ID:?}, \
             got {:?}",
            focus.focused_reason_text
        ));
    }
    if !focus.focus_marker_visible {
        return Err(format!(
            "{DISABLED_ACTION_ID:?} is focused but its gold focus marker is not visible"
        ));
    }
    // A few extra settled frames before capturing, and a retry if the
    // capture looks torn (two consecutive captures disagree): back-to-back
    // `Page.captureScreenshot` calls under headless software rendering were
    // observed to occasionally return a stale/torn frame. This is a
    // best-effort bonus screenshot, not a gated assertion (the checks above
    // already read the ground truth from `FocusSnapshot`), so a capture
    // that still looks torn after retrying is written anyway rather than
    // failing the scenario over a cosmetic artifact.
    for _ in 0..5 {
        let _ = checkpoint.wait_for_frame();
    }
    let mut screenshot = checkpoint
        .screenshot_png(viewport.width, viewport.height)
        .ok();
    for _ in 0..4 {
        let Some(candidate) = &screenshot else { break };
        for _ in 0..5 {
            let _ = checkpoint.wait_for_frame();
        }
        let Ok(next) = checkpoint.screenshot_png(viewport.width, viewport.height) else {
            break;
        };
        if next == *candidate {
            break;
        }
        screenshot = Some(next);
    }
    if let Some(screenshot) = screenshot {
        let _ = artifacts::write_artifact(dir, "focused-disabled.png", screenshot);
    }
    notes.push(format!(
        "disabled reason visible on {:?}: {:?} (marker visible)",
        focus.focused_id, focus.focused_reason_text
    ));
    Ok(())
}

fn wait_for_open_category(
    checkpoint: &Checkpoint,
    category: &str,
) -> Result<PaletteSnapshot, String> {
    for _ in 0..FOCUS_SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_palette(checkpoint)?
            && snapshot
                .phone
                .as_ref()
                .and_then(|phone| phone.open_category.as_deref())
                == Some(category)
        {
            return Ok(snapshot);
        }
    }
    Err(format!(
        "the palette snapshot never reported open_category == {category:?} within \
         {FOCUS_SETTLE_MAX_FRAMES} frames"
    ))
}

fn wait_for_closed_category(checkpoint: &Checkpoint) -> Result<PaletteSnapshot, String> {
    for _ in 0..FOCUS_SETTLE_MAX_FRAMES {
        checkpoint.wait_for_frame()?;
        if let Some(snapshot) = read_palette(checkpoint)?
            && snapshot
                .phone
                .as_ref()
                .is_some_and(|phone| phone.open_category.is_none())
        {
            return Ok(snapshot);
        }
    }
    Err(format!(
        "the palette snapshot never reported a closed category within \
         {FOCUS_SETTLE_MAX_FRAMES} frames"
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

/// Waits for the `Fight` screen and #168's screenshot-stability streak.
fn wait_for_readiness(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
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

        let ready_screen = status.app_booted() && screen.as_deref() == Some("Fight");
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

    let reached_screen = last_screen.as_deref() == Some("Fight");
    let status = match last_status {
        Some(status) => status,
        None => checkpoint.read_status()?,
    };
    let screenshot = last_screenshot.unwrap_or_default();
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
    notes: &[String],
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
            "measured inner: {}x{}\nmeasured client: {}x{}\nscroll: {}x{}\ndevicePixelRatio: {}\ncanvas backing size: {}x{}\nerrors: {:?}\n",
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
    let _ = artifacts::write_artifact(dir, "focus.log", notes.join("\n"));
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
    if screenshot_png.is_empty() {
        problems.push("no screenshot was ever captured (readiness never stabilized)".to_string());
        return;
    }
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
             the fight screen likely never painted (blank canvas)"
        ));
    }
}
