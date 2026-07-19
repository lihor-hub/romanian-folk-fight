//! The `save-reload` scenario (#217, a child of #146): proves the safe-
//! resume-destination journey in a real browser -- result screen -> shop
//! (a safe checkpoint) -> a real page reload -> **Continuă** resumes
//! straight into the shop with every current run value (wallet, ladder
//! position, shop purchase) intact. Extends #168's harness per the
//! documented extension pattern (see `web_smoke::mod`'s module docs): a new
//! module here plus one match arm in `web_smoke::run_scenario` and one entry
//! in `SCENARIOS`.
//!
//! ## What this scenario checks that unit tests cannot
//!
//! `cargo test save::journeys --lib` and `cargo test flow --lib` already
//! exhaustively cover the destination-tagging and resume-intent logic through
//! Bevy's headless `App`. What only a real browser proves: a snapshot
//! actually written to real `localStorage` (`rff_save_v1`) by one browser
//! page load survives a genuine `Page.reload` (a full wasm re-boot, not just
//! an in-memory resource reset) and is read back, restored, and routed to the
//! shop by the *next* page load -- exactly the real-world "player closes the
//! tab and comes back later" journey the issue is about.
//!
//! ## Driving the journey through the review seam
//!
//! Like `gold-journey` (#187), every navigation button press goes through
//! `src/review/mod.rs`'s `pressButton` command so the *production* handler's
//! domain side effect (autosave, run reset, ...) runs before the flow intent
//! it emits -- never a raw `NextState` write. `seedCombat`/`selectPreset`
//! pin the hero and the duel's outcome exactly like `gold-journey`
//! (`GOLD_JOURNEY_SEED`/`Voinicul`, pinned deterministic by
//! `review::gold_journey_seed::gold_journey_seed_wins_the_first_duel` --
//! reused here rather than a second pinned seed, since the matchup is
//! identical). `ShopItem:<name>` (#217, `review::parse_button`) presses one
//! catalog item's real buy/equip button by its stable `ItemId` `Debug` name,
//! exercising the shop-change checkpoint (not just the shop-entry one) before
//! the reload.
//!
//! ## Reading the snapshot directly out of `localStorage`
//!
//! [`SavedRunSnapshot`] mirrors only the fields this scenario needs from
//! `save::snapshot::SaveGame`'s JSON shape (`serde` ignores the rest) --
//! the same partial-mirror approach `corrupt_save_recovery`'s
//! `AccessibilitySnapshot` already uses, since this dev-tooling crate never
//! depends on the game crate's `review` feature (see that module's own doc
//! comment for the reasoning `REVIEW_COMMAND_KEY`'s duplication already
//! documents).
//!
//! ## No screenshot baselines
//!
//! Like `corrupt-save-recovery`/`accessibility-settings-reload`, this
//! scenario's pass/fail gate is exact `localStorage` reads, not a pixel diff.
//! Screenshots are still captured as artifacts for human review.
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-save-reload/`, mirroring every other review-seam
//! scenario, so concurrent scenario runs never clobber each other's build
//! output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, server::StaticServer};

pub const SCENARIO: &str = "save-reload";

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

/// Fixed combat seed for a deterministic, winnable first duel -- identical to
/// `gold_journey::GOLD_JOURNEY_SEED` (same matchup: `Voinicul` vs. the
/// ladder's first opponent), pinned without a browser by
/// `review::gold_journey_seed::gold_journey_seed_wins_the_first_duel`.
const SAVE_RELOAD_SEED: u64 = 20;
/// The creation preset this journey selects -- `HeroPreset::Voinicul`'s exact
/// display name. Starts with `BataCiobaneasca`/`ScutDeLemn`, so
/// `CaciulaDeOaie` (10 galbeni, a head slot Voinicul does not start with) is
/// used for the shop-change checkpoint below.
const SAVE_RELOAD_PRESET: &str = "Voinicul";
/// The item bought at the shop checkpoint, by its stable `ItemId` `Debug`
/// name (see `review::parse_button`'s `ShopItem:<name>` command).
const SHOP_PURCHASE_ITEM: &str = "CaciulaDeOaie";

/// `STARTING_GALBENI` (`progression::STARTING_GALBENI`).
const STARTING_GALBENI: u32 = 50;
/// `fight_reward(1)` for the ladder's level-1 first opponent (`REWARD_BASE`
/// 25 + `REWARD_PER_LEVEL` 10 * level 1).
const FIRST_FIGHT_REWARD: u32 = 35;
/// `CaciulaDeOaie`'s catalog price (`items::catalog::CATALOG`).
const SHOP_PURCHASE_PRICE: u32 = 10;

const BOOT_MAX_FRAMES: usize = 1800;
const BOOT_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);
const SCREEN_MAX_FRAMES: usize = 1800;
const SCREEN_MAX_WALL_CLOCK: Duration = Duration::from_secs(120);
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;
const SETTLE_FRAMES: usize = 10;

/// Exact `headless_chrome` transport signature observed in every #312/#330
/// failure. A surrounding operation label may differ (`wait_for_frame`,
/// `eval`, screenshot, ...), but this library-owned cause proves the CDP
/// transport itself is gone. Product assertions and generic timeout/closed
/// text deliberately do not match.
const CDP_CONNECTION_DEATH_SIGNATURE: &str =
    "Unable to make method calls because underlying connection is closed";
/// A dead CDP connection cannot be recovered in place. The `save-reload`
/// scenario may therefore start one fresh Chrome/profile and replay its whole
/// deterministic journey once; every other error remains fail-fast.
const CDP_CONNECTION_DEATH_RETRY_BUDGET: usize = 1;
const ATTEMPT_CHECKPOINTS: [&str; CDP_CONNECTION_DEATH_RETRY_BUDGET + 1] =
    ["journey", "journey-retry"];

fn is_cdp_connection_death(error: &str) -> bool {
    error.contains(CDP_CONNECTION_DEATH_SIGNATURE)
}

fn run_with_cdp_retry<T, F>(mut run_attempt: F) -> Result<T, String>
where
    F: FnMut(usize) -> Result<T, String>,
{
    let mut first_connection_death = None;

    for attempt in 0..=CDP_CONNECTION_DEATH_RETRY_BUDGET {
        match run_attempt(attempt) {
            Ok(value) => return Ok(value),
            Err(error)
                if is_cdp_connection_death(&error)
                    && attempt < CDP_CONNECTION_DEATH_RETRY_BUDGET =>
            {
                eprintln!(
                    "{SCENARIO}: Chrome/CDP connection died; retrying the complete journey once \
                     with a fresh browser/profile: {error}"
                );
                first_connection_death = Some(error);
            }
            Err(error) if is_cdp_connection_death(&error) => {
                let first = first_connection_death.as_deref().unwrap_or(&error);
                return Err(format!(
                    "Chrome/CDP connection death retry budget exhausted after \
                     {CDP_CONNECTION_DEATH_RETRY_BUDGET} retry; first attempt: {first}; final \
                     attempt: {error}"
                ));
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the bounded attempt loop always returns")
}

pub fn run(update_baselines: bool) -> Result<(), SmokeError> {
    if update_baselines {
        println!(
            "{SCENARIO}: --update-baselines has no effect here -- this scenario has no screenshot baselines (its pass/fail gate is exact localStorage reads, see its module docs)."
        );
    }

    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke {SCENARIO}: serve dist-save-reload/"),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    println!(
        "{SCENARIO}: serving dist-save-reload/ at {}",
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
                "\n{SCENARIO}: result -> shop -> reload resumes at Shop with the wallet, ladder \
                 position, and shop purchase all intact -- artifacts: {}",
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
        .arg("dist-save-reload");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (save-reload)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-save-reload"))
}

fn workspace_root() -> PathBuf {
    crate::process::workspace_root()
}

/// Only the fields this scenario needs from `save::snapshot::SaveGame`'s JSON
/// shape -- `serde` ignores the rest by default (the same partial-mirror
/// approach `corrupt_save_recovery::AccessibilitySnapshot` uses).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
struct SavedRunSnapshot {
    wallet: u32,
    ladder_progress: usize,
    owned_items: Vec<String>,
    equipped: Vec<String>,
    resume_destination: String,
}

fn run_checks(server: &StaticServer) -> Result<(), String> {
    for checkpoint in ATTEMPT_CHECKPOINTS {
        let dir = artifacts::scenario_dir(SCENARIO).join(checkpoint);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to clear stale attempt artifacts {}: {error}",
                    dir.display()
                ));
            }
        }
    }

    run_with_cdp_retry(|attempt| {
        let checkpoint = ATTEMPT_CHECKPOINTS[attempt];
        println!(
            "{SCENARIO}: browser attempt {}/{} -- artifacts: {}",
            attempt + 1,
            ATTEMPT_CHECKPOINTS.len(),
            artifacts::scenario_dir(SCENARIO).join(checkpoint).display()
        );
        run_check_attempt(server, checkpoint)
    })
}

fn run_check_attempt(server: &StaticServer, checkpoint_name: &str) -> Result<(), String> {
    let dir = artifacts::checkpoint_dir(SCENARIO, checkpoint_name)
        .map_err(|e| format!("artifacts dir: {e}"))?;
    let profile_dir = dir.join("chrome-profile");

    let checkpoint =
        browser::launch_with_diagnostics(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, 1.0, &profile_dir)?;
    let outcome = (|| {
        let url = format!("{}/", server.base_url());
        checkpoint.navigate(&url)?;

        // menu -> creation -> fight: seed the duel, start a new game, pick the
        // preset, confirm the hero (autosaves the hero-confirmation checkpoint,
        // resume_destination "fight").
        let (status, _shot) = wait_for_screen(&checkpoint, "MainMenu", true)?;
        check_no_console_or_page_errors(&status, "initial load")?;

        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "seedCombat", "seed": SAVE_RELOAD_SEED}),
        )?;
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
        )?;
        wait_for_screen(&checkpoint, "CharacterCreation", false)?;
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "selectPreset", "preset": SAVE_RELOAD_PRESET}),
        )?;
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
        )?;
        wait_for_screen(&checkpoint, "Fight", false)?;

        let hero_confirm_save = read_saved_run(&checkpoint)?
            .ok_or("no run snapshot after hero confirmation -- the checkpoint never autosaved")?;
        if hero_confirm_save.resume_destination != "fight" {
            return Err(format!(
                "hero confirmation must resume into the arena, saw {:?}",
                hero_confirm_save.resume_destination
            ));
        }

        // fight -> fight-result: autoplay resolves the pinned, winnable duel.
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
        )?;
        wait_for_screen(&checkpoint, "FightResult", false)?;

        let result_save = read_saved_run(&checkpoint)?.ok_or(
            "no run snapshot on the result screen -- the reward checkpoint never autosaved",
        )?;
        if result_save.wallet != STARTING_GALBENI + FIRST_FIGHT_REWARD {
            return Err(format!(
                "result-screen wallet was {}, expected {} (starting {STARTING_GALBENI} + reward \
             {FIRST_FIGHT_REWARD})",
                result_save.wallet,
                STARTING_GALBENI + FIRST_FIGHT_REWARD
            ));
        }
        if result_save.resume_destination != "fight" {
            return Err(format!(
                "the result/reward checkpoint must resume into the arena (matching Lupta \
             următoare), saw {:?}",
                result_save.resume_destination
            ));
        }

        // result -> shop: the shop-entry checkpoint autosaves immediately,
        // switching the resume destination to the shop even before any purchase.
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "GoToShop"}),
        )?;
        wait_for_screen(&checkpoint, "Shop", false)?;

        let shop_entry_save = read_saved_run(&checkpoint)?
            .ok_or("no run snapshot on shop entry -- the shop-entry checkpoint never autosaved")?;
        if shop_entry_save.resume_destination != "shop" {
            return Err(format!(
                "arriving in the shop must resume back into the shop, saw {:?}",
                shop_entry_save.resume_destination
            ));
        }
        if shop_entry_save.wallet != STARTING_GALBENI + FIRST_FIGHT_REWARD {
            return Err(format!(
                "shop-entry wallet changed unexpectedly to {} before any purchase",
                shop_entry_save.wallet
            ));
        }

        // A shop change (#217's "shop changes" checkpoint): buy one affordable,
        // not-yet-owned item, proving the persisted snapshot reflects purchases
        // too, not just arrival.
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": format!("ShopItem:{SHOP_PURCHASE_ITEM}")}),
        )?;
        let expected_wallet_after_purchase =
            STARTING_GALBENI + FIRST_FIGHT_REWARD - SHOP_PURCHASE_PRICE;
        let before_reload = wait_for_wallet(&checkpoint, expected_wallet_after_purchase)?;
        if !before_reload
            .owned_items
            .iter()
            .any(|i| i == SHOP_PURCHASE_ITEM)
        {
            return Err(format!(
                "the purchased item {SHOP_PURCHASE_ITEM:?} is not in owned_items: {:?}",
                before_reload.owned_items
            ));
        }
        if !before_reload
            .equipped
            .iter()
            .any(|i| i == SHOP_PURCHASE_ITEM)
        {
            return Err(format!(
                "the purchased item {SHOP_PURCHASE_ITEM:?} was not auto-equipped: {:?}",
                before_reload.equipped
            ));
        }
        if before_reload.resume_destination != "shop" {
            return Err(format!(
                "a shop purchase must keep resuming into the shop, saw {:?}",
                before_reload.resume_destination
            ));
        }
        if before_reload.ladder_progress != 1 {
            return Err(format!(
                "ladder_progress should be 1 after the first win, saw {}",
                before_reload.ladder_progress
            ));
        }

        let shop_shot = checkpoint.screenshot_png(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
        let _ = artifacts::write_artifact(&dir, "1-shop-before-reload.png", &shop_shot);

        // The real reload: a full wasm re-boot, not an in-memory reset.
        checkpoint.reload()?;
        let (status_after_reload, shot_after_reload) =
            wait_for_screen(&checkpoint, "MainMenu", true)?;
        check_no_console_or_page_errors(&status_after_reload, "after reload")?;
        let _ = artifacts::write_artifact(&dir, "2-main-menu-after-reload.png", &shot_after_reload);

        // Continuă restores the resources and resumes straight into the shop --
        // exactly one flow intent, chosen from the reloaded snapshot's own
        // resume_destination.
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "Continue"}),
        )?;
        let (status_final, shot_final) = wait_for_screen(&checkpoint, "Shop", false)?;
        check_no_console_or_page_errors(&status_final, "after Continuă")?;
        let _ = artifacts::write_artifact(&dir, "3-shop-after-continue.png", &shot_final);

        let after_reload_save = read_saved_run(&checkpoint)?
            .ok_or("no run snapshot survives the reload -- Continuă has nothing to restore")?;
        if after_reload_save != before_reload {
            return Err(format!(
                "the run snapshot changed across the reload: before {before_reload:?}, after \
             {after_reload_save:?}"
            ));
        }

        Ok(())
    })();

    if let Err(error) = &outcome
        && let Err(diagnostic_error) = checkpoint.write_process_diagnostics(error)
    {
        eprintln!(
            "{SCENARIO}: failed to retain Chrome process evidence without replacing the original \
             failure: {diagnostic_error}"
        );
    }

    outcome
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

fn read_saved_run(checkpoint: &Checkpoint) -> Result<Option<SavedRunSnapshot>, String> {
    match read_local_storage_item(checkpoint, SAVE_STORAGE_KEY)? {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("stored run snapshot was not valid JSON ({json}): {e}")),
    }
}

/// Polls the stored run snapshot until its wallet reads `expected` (the
/// purchase autosave landing), settles a few more frames, and returns it --
/// bounded by [`SCREEN_MAX_FRAMES`]/[`SCREEN_MAX_WALL_CLOCK`].
fn wait_for_wallet(checkpoint: &Checkpoint, expected: u32) -> Result<SavedRunSnapshot, String> {
    let start = Instant::now();
    let mut last: Option<SavedRunSnapshot> = None;
    for _ in 0..SCREEN_MAX_FRAMES {
        if start.elapsed() > SCREEN_MAX_WALL_CLOCK {
            break;
        }
        checkpoint.wait_for_frame()?;
        if let Some(save) = read_saved_run(checkpoint)? {
            if save.wallet == expected {
                for _ in 0..SETTLE_FRAMES {
                    checkpoint.wait_for_frame()?;
                }
                return read_saved_run(checkpoint)?.ok_or_else(|| {
                    "run snapshot disappeared while settling after the purchase".to_string()
                });
            }
            last = Some(save);
        }
    }
    Err(format!(
        "the stored wallet never reached {expected} within {SCREEN_MAX_WALL_CLOCK:?} (last seen: \
         {last:?})"
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

#[cfg(test)]
mod tests {
    use super::*;

    const OBSERVED_CDP_DEATH: &str = "waiting for an animation frame failed: Unable to make method calls because underlying connection is closed";

    #[test]
    fn observed_headless_chrome_connection_death_is_retryable() {
        assert!(is_cdp_connection_death(OBSERVED_CDP_DEATH));
    }

    #[test]
    fn assertions_and_near_match_transport_errors_are_not_retryable() {
        for error in [
            "the run snapshot changed across the reload",
            "page reload failed: websocket connection closed",
            "Unable to make method calls because connection is closed",
            "Unable to make method calls because underlying connection is open",
            "unable to make method calls because underlying connection is closed",
        ] {
            assert!(
                !is_cdp_connection_death(error),
                "near-match error must remain fail-fast: {error}"
            );
        }
    }

    #[test]
    fn one_cdp_death_gets_exactly_one_fresh_attempt() {
        let mut attempts = Vec::new();
        let result = run_with_cdp_retry(|attempt| {
            attempts.push(attempt);
            if attempt == 0 {
                Err(OBSERVED_CDP_DEATH.to_string())
            } else {
                Ok("passed")
            }
        });

        assert_eq!(result.as_deref(), Ok("passed"));
        assert_eq!(attempts, vec![0, 1]);
    }

    #[test]
    fn repeated_cdp_death_exhausts_the_single_retry_budget() {
        let mut attempts = Vec::new();
        let error = run_with_cdp_retry::<(), _>(|attempt| {
            attempts.push(attempt);
            Err(format!("attempt {attempt}: {OBSERVED_CDP_DEATH}"))
        })
        .expect_err("a second CDP death must fail the scenario");

        assert_eq!(attempts, vec![0, 1]);
        assert!(error.contains("retry budget exhausted"));
        assert!(error.contains("attempt 0"));
        assert!(error.contains("attempt 1"));
    }

    #[test]
    fn an_assertion_failure_is_never_retried() {
        let mut attempts = Vec::new();
        let error = run_with_cdp_retry::<(), _>(|attempt| {
            attempts.push(attempt);
            Err("the run snapshot changed across the reload".to_string())
        })
        .expect_err("the product assertion must fail immediately");

        assert_eq!(attempts, vec![0]);
        assert_eq!(error, "the run snapshot changed across the reload");
    }

    #[test]
    fn an_assertion_after_the_transport_retry_does_not_get_a_third_attempt() {
        let mut attempts = Vec::new();
        let error = run_with_cdp_retry::<(), _>(|attempt| {
            attempts.push(attempt);
            if attempt == 0 {
                Err(OBSERVED_CDP_DEATH.to_string())
            } else {
                Err("stored wallet never reached 75".to_string())
            }
        })
        .expect_err("the retry's product assertion must fail immediately");

        assert_eq!(attempts, vec![0, 1]);
        assert_eq!(error, "stored wallet never reached 75");
    }
}
