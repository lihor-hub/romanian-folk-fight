//! The `gold-journey` scenario (#187, a child of #144): drives the full
//! menu -> creation -> first fight -> fight result -> shop loop
//! deterministically (a "gold run": the player wins the first duel and
//! reaches the shop) and captures a checkpoint at each screen, at both
//! desktop and phone viewports (10 captures total). Extends #168's
//! `cold-menu` harness per its documented extension pattern (see
//! `web_smoke::mod`'s module docs): a new module here plus one match arm in
//! `web_smoke::run_scenario`; the shared `browser`/`server`/`artifacts`/
//! `baseline` building blocks are reused as-is (with two small additive
//! methods on `browser::Checkpoint` -- `eval_unit`/`eval_string` -- for
//! talking to the review seam below; `cold-menu` never needed them).
//!
//! ## The review seam this scenario drives
//!
//! `src/review/mod.rs` (compiled only behind the `review` cargo feature,
//! never in an ordinary build -- see that module's docs for the full
//! contract and why it is structurally unreachable otherwise) exposes a
//! `window.localStorage`-based bridge:
//!
//! - **Commands** (this scenario -> the game): a JSON object written to
//!   [`REVIEW_COMMAND_KEY`] via [`send_command`], mirroring
//!   `review::ReviewCommand`'s wire format (duplicated here as plain string
//!   literals, not shared code -- this dev-tooling crate never depends on
//!   the game crate's `review` feature, the same reasoning
//!   `cold_menu::REQUIRED_ASSETS` documents for `core::UI_FONT_PATH`):
//!   `seedCombat` (fixes the duel's RNG before the fight starts),
//!   `selectPreset` (picks a named creation preset, exactly like clicking
//!   its button), `pressButton` (sets `Interaction::Pressed` on the named
//!   screen button's real entity, so the *production* click handler runs --
//!   domain side effects like the run reset and the `PlayerCharacter`
//!   insert included -- and emits the flow intent itself), `setAutoplay`
//!   (scripts the player's combat turns with a fixed, deterministic policy
//!   once the fresh fight-start checkpoint is captured), and
//!   `setTimePaused` (freezes `Time<Virtual>` around each capture so
//!   screens with continuous idle animation -- the fight screen's parallax
//!   drift and idle sprite frames -- can satisfy the byte-identical-frames
//!   stability streak), and `advanceTime` (#272; jumps `Time<Virtual>`
//!   forward by a fixed amount in one step, right before `setTimePaused`, so
//!   any bounded, time-driven reveal animation is unambiguously finished
//!   before the freeze -- see [`SETTLE_TIME_ADVANCE_SECONDS`] and
//!   [`captured_checkpoint`]).
//! - **Readiness** (the game -> this scenario): the current `GameState`'s
//!   `Debug` name, published every frame to [`REVIEW_SCREEN_KEY`]. Every
//!   checkpoint here waits for that exact screen name *and* #168's existing
//!   frame-stability contract (`Checkpoint::wait_for_frame` + a
//!   byte-identical-screenshot streak) before it is considered ready -- no
//!   fixed sleeps anywhere in this module.
//!
//! ## Determinism
//!
//! [`GOLD_JOURNEY_SEED`] fixes the duel's RNG (`seedCombat`) and
//! [`GOLD_JOURNEY_PRESET`] fixes the hero (Voinicul, agilitate 3, opens the
//! round against the ladder's first opponent, Hoț de codru, agilitate 2).
//! `src/review/mod.rs`'s `gold_journey_seed_wins_the_first_duel` test pins,
//! with the pure `combat::engine`/`combat::ai` functions directly (no
//! browser needed), that this exact seed + preset + the fixed autoplay
//! policy (`autoplay_player_turn`: Rest below the quick-strike stamina cost,
//! QuickStrike otherwise) defeats that opponent within a small, fixed number
//! of turns -- so the whole journey's *state* (the checkpoint sequence, the
//! duel's outcome, the wallet/reward numbers on the result and shop screens)
//! is reproducible run to run. Screenshot determinism (forced software
//! rendering, per `browser`'s module docs) holds byte-for-byte for the
//! static screens; on screens with idle animation (fight, and any screen
//! showing the animated cutout preview), the *phase* the `setTimePaused`
//! freeze lands on depends on wall-clock elapsed since boot, so a re-run's
//! screenshot can differ by that animation phase. This is the documented
//! tolerance: per `baseline`'s policy (unchanged from #168), a baseline
//! mismatch is reported with a pixel count but is not by itself a failure --
//! the hard assertions (console errors, assets, scroll, blank paint) gate
//! identically on every run. This tolerance is deliberate and stays scoped
//! to *unbounded, perpetual* idle motion (arena parallax/idle sprite frames)
//! -- it is not a license for a *bounded, one-shot* reveal animation to
//! settle at a different frame on every run, which is exactly what #272's
//! settling step (below) rules out.
//!
//! ## Settling bounded reveal animations before capture (#272)
//!
//! The `fight-result` and `shop` checkpoints render a static breakdown (no
//! fighters, no perpetual motion), so unlike `fight` they are expected to be
//! byte-identical run to run -- but #168's byte-identical-frames streak alone
//! cannot *guarantee* that: a value that renders as a rounded/quantized pixel
//! image while still animating toward its final total (a count-up reveal,
//! for instance) can hold the *same* rendered pixels across several
//! consecutive frames purely by that quantization, satisfying the streak by
//! coincidence at whatever fraction of the animation's duration the harness
//! happened to be polling -- observed in practice as a checkpoint captured at
//! ~14%/~65% progress on different runs. [`captured_checkpoint`] closes this
//! gap generically (every checkpoint, not a `fight-result`/`shop` special
//! case) by sending `advanceTime` with [`SETTLE_TIME_ADVANCE_SECONDS`] right
//! before `setTimePaused`: a single jump far longer than any plausible
//! bounded animation on these screens, so whatever the streak already
//! measured as "stable," any such animation is *unambiguously* finished
//! (not just quantized-still-for-a-moment) by the time the clock actually
//! freezes and the screenshot is taken. This does not, and is not intended
//! to, change the `fight` checkpoint's own unbounded-motion tolerance above:
//! jumping time shifts a perpetual parallax drift's phase by a fixed amount,
//! but the *starting* phase at the moment `fight` is reached is still
//! wall-clock-dependent, so the resulting phase stays non-deterministic
//! either way -- `fight`'s pixel-count tolerance is unaffected by this
//! change, by design (see the issue's own scope note on the `fight` cell).
//!
//! ## Separate build, separate `dist/`
//!
//! [`build_review_release`] runs `trunk build --release --features review`
//! into its own `dist-gold-journey/` directory (never `dist/`, which
//! `cold-menu` builds without the `review` feature) -- so a `cold-menu` run
//! and a `gold-journey` run never clobber each other's build output, and the
//! ordinary release artifact is never the one this scenario serves.
//!
//! ## Checkpoints and the DPR matrix (#198)
//!
//! Five semantic checkpoints -- `menu`, `creation`, `fight`, `fight-result`,
//! `shop` -- each captured at both `desktop` (1280x800) and `phone`
//! (390x844), at device pixel ratios 1, 2, and 3 ([`VIEWPORTS`]): 6
//! viewport entries x 5 checkpoints = 30 captures. DPR is applied per-tab
//! over CDP (`Emulation.setDeviceMetricsOverride`'s `device_scale_factor`,
//! see `browser::launch`'s doc comment) -- the CSS-pixel viewport
//! (`window.innerWidth/innerHeight`) stays exactly `1280x800`/`390x844` at
//! every DPR, while the captured screenshot's physical pixel dimensions
//! scale by `dpr` (asserted in `check_screenshot_pixels`). Baselines live at
//! `tests/visual/baselines/gold-journey/<viewport>-<checkpoint>.png`, where
//! `<viewport>` is `desktop`/`phone` at DPR 1 (the pre-#198 names, kept
//! valid rather than migrated -- see [`VIEWPORTS`]'s doc comment) or
//! `desktop-dpr2`/`phone-dpr3`/etc. at DPR 2/3.
//!
//! Every checkpoint reuses the same class of assertions #168 established
//! (adapted per-checkpoint below, not shared code -- see the module docs
//! above for why): no console/page errors, every required asset fetched
//! with a 2xx status, no unexpected document scroll, an exact DPR-scaled
//! screenshot size, and a screenshot that is neither blank nor an
//! untextured white placeholder. The `shop` checkpoint additionally
//! requires the six shop icon assets (fetched once at `PreStartup`, same as
//! the font/panel, since this is a single continuous page load across every
//! checkpoint -- there is no per-screen navigation reload to re-fetch
//! anything).
//!
//! ## Visual-diff gating (#198)
//!
//! A checkpoint whose screenshot differs from its accepted baseline stays
//! non-fatal by default (per `baseline`'s policy) but always gets an
//! `actual`/`expected`/`diff` PNG triplet written into that checkpoint's
//! artifact directory (`baseline::write_diff_triplet`), so CI can upload a
//! focused, reviewable bundle. Passing `--strict-visual` (or setting
//! `XTASK_WEB_SMOKE_STRICT_VISUAL=1`) turns that diff into an explicit
//! checkpoint failure instead.
//!
//! Each viewport gets its own fresh Chrome profile (a clean, empty
//! `localStorage`/cache -- the "clean profile" the issue asks for) and its
//! own single, continuous browser session that walks all five checkpoints
//! in order, since the whole point is one player's journey through the
//! screens, not five independent cold loads.
//!
//! ## Desktop-only default scope (#284)
//!
//! Agent/CI feedback is dominated by this scenario's wall-clock cost, most
//! of which comes from walking the phone and DPR-2/3 legs of the matrix
//! above. For current development, [`active_viewports`] defaults to a
//! single entry -- [`ACTIVE_VIEWPORTS`], `desktop` at 1280x800/DPR 1 -- so
//! an ordinary `--scenario gold-journey` run walks exactly the five
//! [`CHECKPOINTS`] screens (menu, creation, fight, fight-result, shop) once,
//! at the one viewport current development cares about (5 checkpoints, not
//! 30). Nothing about the matrix itself is deleted: [`FULL_VIEWPORTS`] is
//! the exact, unchanged six-entry table documented above, still reachable
//! by setting `XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX=1` (or `true`,
//! case-insensitive) in the environment before invoking the scenario --
//! see [`resolve_viewports`] for the selection logic (unit-tested below) and
//! `.github/workflows/web-smoke.yml`'s `gold-journey` job / its manual
//! `workflow_dispatch` `full_matrix` input for how CI reactivates it on
//! demand. Every baseline this scenario reads/writes is unaffected: the
//! narrowed default only changes which of the existing
//! `tests/visual/baselines/gold-journey/<viewport>-<checkpoint>.png` files
//! get exercised on a given run, never their contents or names. This is a
//! deliberately temporary scope (see issue #284's body) -- reactivating the
//! broader coverage is a one-line env var, not a code change.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::process::run_step;
use crate::web_smoke::browser::{self, Checkpoint, PageStatus};
use crate::web_smoke::error::SmokeError;
use crate::web_smoke::{artifacts, baseline, server::StaticServer};

pub const SCENARIO: &str = "gold-journey";

/// `localStorage` key this scenario writes pending review commands to.
/// Mirrors `crate::review::REVIEW_COMMAND_KEY` in the *game* crate -- kept
/// as a plain duplicated string literal rather than shared code; see the
/// module docs above for why.
const REVIEW_COMMAND_KEY: &str = "rff_review_cmd_v1";
/// `localStorage` key the game publishes the current screen's name to.
/// Mirrors `crate::review::REVIEW_SCREEN_KEY`.
const REVIEW_SCREEN_KEY: &str = "rff_review_screen_v1";

/// Fixed combat seed for a deterministic, winnable first duel. Kept equal to
/// `src/review/mod.rs`'s `gold_journey_seed::GOLD_JOURNEY_SEED`, which pins
/// (via the pure combat engine/AI, no browser) that this exact seed wins.
const GOLD_JOURNEY_SEED: u64 = 20;
/// The creation preset this journey selects -- `HeroPreset::Voinicul`'s
/// exact display name (see `creation::draft::HeroPreset::name`), agilitate 3
/// so it opens the round against the ladder's first opponent (agilitate 2).
const GOLD_JOURNEY_PRESET: &str = "Voinicul";

/// `(fetch path suffix, repo-relative source file)` -- identical to
/// `cold_menu::REQUIRED_ASSETS`; duplicated rather than shared, per that
/// module's own precedent (a later scenario is its own module, mirroring
/// `cold_menu.rs`, not a refactor of it).
const BASE_REQUIRED_ASSETS: &[(&str, &str)] = &[
    (
        "assets/fonts/Alegreya-Variable.ttf",
        "assets/fonts/Alegreya-Variable.ttf",
    ),
    ("assets/ui/panel_border.png", "assets/ui/panel_border.png"),
];

/// The six shop icon assets (`shop::shop_icon_path`), fetched once at
/// `PreStartup` like the font/panel -- checked only from the `shop`
/// checkpoint on, since that is the screen whose visual contract depends on
/// them (see the module docs' broken-asset fixture note).
const SHOP_ICON_ASSETS: &[(&str, &str)] = &[
    ("assets/ui/icon_coin.png", "assets/ui/icon_coin.png"),
    ("assets/ui/icon_weapon.png", "assets/ui/icon_weapon.png"),
    ("assets/ui/icon_shield.png", "assets/ui/icon_shield.png"),
    ("assets/ui/icon_torso.png", "assets/ui/icon_torso.png"),
    ("assets/ui/icon_head.png", "assets/ui/icon_head.png"),
    ("assets/ui/icon_feet.png", "assets/ui/icon_feet.png"),
];

struct ViewportSpec {
    name: &'static str,
    width: u32,
    height: u32,
    dpr: f64,
}

/// The full #198 matrix: both base viewports (desktop 1280x800, phone
/// 390x844) at device pixel ratios 1, 2, and 3 -- six viewport entries, each
/// walking all five [`CHECKPOINTS`] (30 captures total).
///
/// ## Checkpoint naming (#198)
///
/// `capture`'s `checkpoint_key` is `format!("{}-{}", viewport.name,
/// spec.name)`, unchanged from #187 -- so the DPR dimension is folded
/// entirely into `ViewportSpec::name` here rather than touching that
/// formatting code. DPR 1 keeps the exact pre-#198 names (`desktop`,
/// `phone`), so every baseline #187 already committed
/// (`tests/visual/baselines/gold-journey/{desktop,phone}-{menu,creation,
/// fight,fight-result,shop}.png`) stays valid without a rename/migration.
/// DPR 2 and 3 extend the convention with a `-dprN` suffix (`desktop-dpr2`,
/// `phone-dpr3`, ...), giving checkpoint keys like `desktop-dpr2-fight` --
/// exactly the naming the issue asks for. This was a deliberate choice over
/// migrating every viewport to an explicit `-dpr1` suffix: it keeps the
/// existing, human-reviewed DPR-1 baselines' git history and filenames
/// untouched (no `--update-baselines` run needed just to rename files), at
/// the cost of the naming convention being non-uniform (`desktop` implies
/// DPR 1; `desktop-dpr2` is explicit). See `xtask/README.md`'s "web-smoke
/// --all"/matrix section for the full table.
const FULL_VIEWPORTS: &[ViewportSpec] = &[
    ViewportSpec {
        name: "desktop",
        width: 1280,
        height: 800,
        dpr: 1.0,
    },
    ViewportSpec {
        name: "desktop-dpr2",
        width: 1280,
        height: 800,
        dpr: 2.0,
    },
    ViewportSpec {
        name: "desktop-dpr3",
        width: 1280,
        height: 800,
        dpr: 3.0,
    },
    ViewportSpec {
        name: "phone",
        width: 390,
        height: 844,
        dpr: 1.0,
    },
    ViewportSpec {
        name: "phone-dpr2",
        width: 390,
        height: 844,
        dpr: 2.0,
    },
    ViewportSpec {
        name: "phone-dpr3",
        width: 390,
        height: 844,
        dpr: 3.0,
    },
];

/// #284's narrowed default: the single `desktop` (1280x800 @ DPR 1) entry
/// from [`FULL_VIEWPORTS`], covering the five [`CHECKPOINTS`] screens at the
/// one viewport the active development/CI feedback loop currently exercises.
/// See "Desktop-only default scope (#284)" in the module docs above.
const ACTIVE_VIEWPORTS: &[ViewportSpec] = &[ViewportSpec {
    name: "desktop",
    width: 1280,
    height: 800,
    dpr: 1.0,
}];

/// Set to `1`/`true` (case-insensitive; anything else, including unset, is
/// treated as not-enabled) to opt into the full six-viewport [`FULL_VIEWPORTS`]
/// matrix instead of the narrowed [`ACTIVE_VIEWPORTS`] default -- the #284
/// reactivation path. Read by [`active_viewports`].
const FULL_MATRIX_ENV_VAR: &str = "XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX";

/// Pure selection logic behind [`active_viewports`], split out so it can be
/// unit-tested without mutating the process-wide environment (tests run
/// concurrently in the same process, so racing `std::env::set_var` calls
/// would be flaky -- same reasoning as `crate::process::resolve_budget_ms`).
fn resolve_viewports(full_matrix_env: Option<&str>) -> &'static [ViewportSpec] {
    let full_matrix_requested = full_matrix_env
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if full_matrix_requested {
        FULL_VIEWPORTS
    } else {
        ACTIVE_VIEWPORTS
    }
}

/// The viewport set this run actually walks: [`ACTIVE_VIEWPORTS`] (desktop
/// DPR 1 only) by default, or the full [`FULL_VIEWPORTS`] matrix when
/// [`FULL_MATRIX_ENV_VAR`] is set to `1`/`true` (#284).
fn active_viewports() -> &'static [ViewportSpec] {
    resolve_viewports(std::env::var(FULL_MATRIX_ENV_VAR).ok().as_deref())
}

/// One semantic checkpoint: its name, the `GameState` it must observe
/// (`review::publish_current_screen`'s exact `Debug` output), and any
/// required assets beyond [`BASE_REQUIRED_ASSETS`].
struct CheckpointSpec {
    name: &'static str,
    expected_screen: &'static str,
    extra_required_assets: &'static [(&'static str, &'static str)],
}

const CHECKPOINTS: &[CheckpointSpec] = &[
    CheckpointSpec {
        name: "menu",
        expected_screen: "MainMenu",
        extra_required_assets: &[],
    },
    CheckpointSpec {
        name: "creation",
        expected_screen: "CharacterCreation",
        extra_required_assets: &[],
    },
    CheckpointSpec {
        name: "fight",
        expected_screen: "Fight",
        extra_required_assets: &[],
    },
    CheckpointSpec {
        name: "fight-result",
        expected_screen: "FightResult",
        extra_required_assets: &[],
    },
    CheckpointSpec {
        name: "shop",
        expected_screen: "Shop",
        extra_required_assets: SHOP_ICON_ASSETS,
    },
];

const READY_MAX_FRAMES: usize = 3600;
const READY_MAX_WALL_CLOCK: Duration = Duration::from_secs(180);
const STABLE_FRAMES_REQUIRED: usize = 3;

/// Sent as an `advanceTime` command right before every checkpoint's
/// `setTimePaused` freeze (#272; see the module docs' "Settling bounded
/// reveal animations before capture" section). Five in-game seconds is
/// comfortably longer than any plausible bounded, time-driven reveal
/// animation on these screens (for reference, `progression::FIGHT_END_DELAY_SECONDS`
/// -- a comparable fixed-duration timer on the game side -- runs 1.5
/// seconds), so the jump guarantees such an animation has reached its
/// terminal frame before the clock freezes, regardless of which real-world
/// frame the byte-identical-frames streak happened to land the freeze on
/// beforehand.
const SETTLE_TIME_ADVANCE_SECONDS: f32 = 5.0;

pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    let dist_dir = build_review_release()?;
    let server = StaticServer::start(dist_dir).map_err(|e| {
        SmokeError::scenario(
            "web-smoke: serve dist-gold-journey/",
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    let viewports = active_viewports();
    println!(
        "gold-journey: serving dist-gold-journey/ at {} ({} viewport(s) x {} screen(s) = {} checkpoint(s){})",
        server.base_url(),
        viewports.len(),
        CHECKPOINTS.len(),
        viewports.len() * CHECKPOINTS.len(),
        if viewports.len() == FULL_VIEWPORTS.len() {
            ""
        } else {
            " -- desktop-only default (#284); set \
             XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX=1 for the full matrix"
        }
    );

    let mut missing_baseline = false;
    for viewport in viewports {
        run_viewport_journey(
            viewport,
            &server,
            update_baselines,
            strict_visual,
            &mut missing_baseline,
        )?;
    }

    if update_baselines {
        println!(
            "\ngold-journey: baselines updated at tests/visual/baselines/{SCENARIO}/ for {} checkpoint(s).",
            CHECKPOINTS.len() * viewports.len()
        );
    } else if missing_baseline {
        println!(
            "\ngold-journey: no accepted baseline existed yet for one or more checkpoints -- \
             the non-screenshot assertions above still ran and passed. Re-run with \
             --update-baselines once you've reviewed the captured screenshots to accept them."
        );
    } else {
        println!("\ngold-journey: all checkpoints passed against their accepted baselines.");
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
        .arg("dist-gold-journey");
    cmd.current_dir(workspace_root());
    run_step(
        "web-smoke: trunk build --release --features review (gold-journey)",
        cmd,
    )?;
    Ok(workspace_root().join("dist-gold-journey"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml always has a parent workspace root")
        .to_path_buf()
}

/// Writes one review command (see the module docs) as JSON into
/// [`REVIEW_COMMAND_KEY`]. `payload` must already be a JSON object literal
/// (built with `serde_json::json!`).
///
/// Also waits for the game to actually drain the command (`review::poll_review_commands`
/// consumes at most one per frame, clearing the key immediately after
/// reading it) before returning. This matters because this scenario issues
/// several commands back-to-back (e.g. `seedCombat` then `pressButton`
/// `NewGame`) with no other synchronization between them -- without
/// waiting here, a second `localStorage.setItem` could silently overwrite
/// the first command before the game ever reads it.
const COMMAND_CONSUMED_MAX_FRAMES: usize = 300;

fn send_command(checkpoint: &Checkpoint, payload: serde_json::Value) -> Result<(), String> {
    let json = payload.to_string();
    // `serde_json::to_string` on a `String` produces a double-quoted,
    // escaped literal -- valid JSON string syntax, which is also valid JS
    // string syntax, so it can be embedded directly into the script below.
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

/// Runs one full viewport's journey through all five checkpoints in a
/// single, continuous browser session (one fresh Chrome profile -- the
/// "clean profile" the issue asks for -- shared across every checkpoint of
/// this viewport, since it is one player's journey through the screens, not
/// five independent cold loads).
fn run_viewport_journey(
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

    let checkpoint = browser::launch(viewport.width, viewport.height, viewport.dpr, &profile_dir)
        .map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke gold-journey[{}]", viewport.name),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;
    let url = format!("{}/", server.base_url());
    checkpoint.navigate(&url).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke gold-journey[{}]", viewport.name),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    // menu: nothing to seed yet, just wait for the cold boot.
    captured_checkpoint(
        &checkpoint,
        viewport,
        &CHECKPOINTS[0],
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    // Seed combat before the fight starts -- `combat::systems::setup_combat`
    // only seeds `CombatRng` from the clock when the resource is absent, so
    // this must land before the hero is confirmed.
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "seedCombat", "seed": GOLD_JOURNEY_SEED}),
        )
    })?;
    // Press the real "Luptă nouă" button: its production handler resets the
    // run, then emits StartNewGame.
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "NewGame"}),
        )
    })?;
    // creation: select the preset once we've arrived, then capture with the
    // preset's name/stats/preview visible.
    wait_for_checkpoint(&checkpoint, &CHECKPOINTS[1], viewport)?;
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "selectPreset", "preset": GOLD_JOURNEY_PRESET}),
        )
    })?;
    captured_checkpoint(
        &checkpoint,
        viewport,
        &CHECKPOINTS[1],
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    // Press the real "Începe lupta" button: its production handler stores
    // the PlayerCharacter + starter loadout, requests the autosave, then
    // emits ConfirmHero -- a raw flow intent would skip all of that and the
    // arena would never spawn (`arena::mod` warns and bails without a
    // PlayerCharacter).
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "ConfirmHero"}),
        )
    })?;
    // fight: capture the fresh fight start before autoplay drives it to a
    // resolution, so this checkpoint always shows both fighters at full
    // health regardless of how fast the duel resolves.
    captured_checkpoint(
        &checkpoint,
        viewport,
        &CHECKPOINTS[2],
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "setAutoplay", "enabled": true}),
        )
    })?;
    // fight-result: autoplay resolves the duel (deterministically, per the
    // module docs) and progression's own fight-end delay emits the automated
    // ResolveVictory intent -- no command to send, just wait.
    captured_checkpoint(
        &checkpoint,
        viewport,
        &CHECKPOINTS[3],
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    // Press the real "La prăvălie" button on the result screen.
    step(viewport, || {
        send_command(
            &checkpoint,
            serde_json::json!({"cmd": "pressButton", "button": "GoToShop"}),
        )
    })?;
    captured_checkpoint(
        &checkpoint,
        viewport,
        &CHECKPOINTS[4],
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    )?;

    Ok(())
}

/// One captured checkpoint: reach the expected screen with time running
/// (transitions, the fight-end delay, and autoplay all need the clock), send
/// `advanceTime` to deterministically settle any bounded reveal animation to
/// its terminal frame (#272 -- see the module docs' "Settling bounded reveal
/// animations before capture" section), then freeze `Time<Virtual>` so idle
/// animation can't defeat the byte-identical-frames stability streak,
/// capture + assert + baseline, and unfreeze for the journey's next leg.
fn captured_checkpoint(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    spec: &CheckpointSpec,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    wait_for_checkpoint(checkpoint, spec, viewport)?;
    // #272: jump the clock forward before freezing it, so a bounded,
    // time-driven reveal animation still in flight when the byte-identical-
    // frames streak above happened to settle (possibly by quantization
    // coincidence, mid-animation) is unambiguously finished by the time the
    // capture below actually runs.
    step(viewport, || {
        send_command(
            checkpoint,
            serde_json::json!({"cmd": "advanceTime", "seconds": SETTLE_TIME_ADVANCE_SECONDS}),
        )
    })?;
    step(viewport, || {
        send_command(
            checkpoint,
            serde_json::json!({"cmd": "setTimePaused", "paused": true}),
        )
    })?;
    let result = capture(
        checkpoint,
        viewport,
        spec,
        server,
        update_baselines,
        strict_visual,
        missing_baseline,
    );
    // Unpause even when the capture failed, so a debugging session against
    // the still-open browser (or a later checkpoint's diagnostics pass)
    // isn't stuck on a frozen clock.
    let unpause = step(viewport, || {
        send_command(
            checkpoint,
            serde_json::json!({"cmd": "setTimePaused", "paused": false}),
        )
    });
    result.and(unpause)
}

/// Runs a fallible step (typically [`send_command`]) and wraps a failure
/// into a [`SmokeError`] pointing at the viewport's artifact directory.
fn step(
    viewport: &ViewportSpec,
    action: impl FnOnce() -> Result<(), String>,
) -> Result<(), SmokeError> {
    action().map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke gold-journey[{}]", viewport.name),
            e,
            artifacts::scenario_dir(SCENARIO),
        )
    })
}

/// Waits for [`CheckpointSpec::expected_screen`] and #168's stability
/// contract, without capturing/asserting -- used when this scenario needs to
/// reach a screen before issuing a further command (e.g. reaching
/// `CharacterCreation` before selecting the preset) but the checkpoint
/// itself is captured afterward.
fn wait_for_checkpoint(
    checkpoint: &Checkpoint,
    spec: &CheckpointSpec,
    viewport: &ViewportSpec,
) -> Result<(), SmokeError> {
    wait_for_readiness(checkpoint, spec, viewport)
        .map(|_| ())
        .map_err(|e| {
            SmokeError::scenario(
                format!("web-smoke gold-journey[{}][{}]", viewport.name, spec.name),
                e,
                artifacts::scenario_dir(SCENARIO),
            )
        })
}

/// Waits for the checkpoint to be ready, runs every assertion, writes
/// artifacts unconditionally, and compares/updates the baseline -- the
/// gold-journey analogue of `cold_menu::run_checkpoint`.
fn capture(
    checkpoint: &Checkpoint,
    viewport: &ViewportSpec,
    spec: &CheckpointSpec,
    server: &StaticServer,
    update_baselines: bool,
    strict_visual: bool,
    missing_baseline: &mut bool,
) -> Result<(), SmokeError> {
    let checkpoint_key = format!("{}-{}", viewport.name, spec.name);
    let dir = artifacts::checkpoint_dir(SCENARIO, &checkpoint_key).map_err(|e| {
        SmokeError::scenario(
            format!("web-smoke gold-journey[{checkpoint_key}]"),
            e.to_string(),
            artifacts::scenario_dir(SCENARIO),
        )
    })?;

    let outcome = wait_for_readiness(checkpoint, spec, viewport);
    let (status, screenshot, readiness) = match outcome {
        Ok(triple) => triple,
        Err(message) => {
            let _ = artifacts::write_artifact(&dir, "server.log", server.request_log().join("\n"));
            return Err(SmokeError::scenario(
                format!("web-smoke gold-journey[{checkpoint_key}]"),
                message,
                dir,
            ));
        }
    };

    write_artifacts(&dir, viewport, &status, &screenshot, server);

    let mut problems = Vec::new();
    if !readiness.reached_screen {
        problems.push(format!(
            "never observed screen `{}` within {:?}/{} frames (last seen: {:?})",
            spec.expected_screen, READY_MAX_WALL_CLOCK, READY_MAX_FRAMES, readiness.last_screen
        ));
    } else if !readiness.stabilized {
        problems.push(format!(
            "first paint never stabilized on screen `{}` within {:?}/{} frames ({} observed)",
            spec.expected_screen, READY_MAX_WALL_CLOCK, READY_MAX_FRAMES, readiness.frames_observed
        ));
    } else {
        check_no_console_or_page_errors(&status, &mut problems);
        check_required_assets(spec, &status, &mut problems);
        check_no_unexpected_scroll(viewport, &status, &mut problems);
        check_screenshot_pixels(viewport, &screenshot, &mut problems);
    }

    if !problems.is_empty() {
        let message = format!(
            "gold-journey[{checkpoint_key}] ({}x{}, ready in {:?}, {} frame(s)) failed:\n  - {}",
            viewport.width,
            viewport.height,
            readiness.elapsed,
            readiness.frames_observed,
            problems.join("\n  - ")
        );
        return Err(SmokeError::scenario(
            format!("web-smoke gold-journey[{checkpoint_key}]"),
            message,
            dir,
        ));
    }

    match baseline::handle(SCENARIO, &checkpoint_key, &screenshot, update_baselines) {
        Ok(baseline::BaselineOutcome::Updated) => {
            println!(
                "gold-journey[{checkpoint_key}]: OK -- baseline updated at {} -- artifacts: {}",
                baseline::baseline_path(SCENARIO, &checkpoint_key).display(),
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Missing) => {
            *missing_baseline = true;
            println!(
                "gold-journey[{checkpoint_key}]: OK -- no baseline exists yet -- artifacts: {}",
                dir.display()
            );
        }
        Ok(baseline::BaselineOutcome::Matches) => {
            println!(
                "gold-journey[{checkpoint_key}]: OK -- matches accepted baseline -- artifacts: {}",
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
                    "gold-journey[{checkpoint_key}] ({}x{}) failed:\n  - screenshot differs from accepted \
                     baseline ({diff_pixels}/{total_pixels} px) under --strict-visual",
                    viewport.width, viewport.height
                );
                if let Ok(paths) = &diff_paths {
                    message.push_str(&format!("\n  diff triplet: {}", paths.describe()));
                }
                return Err(SmokeError::scenario(
                    format!("web-smoke gold-journey[{checkpoint_key}]"),
                    message,
                    dir,
                ));
            }
            println!(
                "gold-journey[{checkpoint_key}]: OK -- differs from accepted baseline ({diff_pixels}/{total_pixels} px; \
                 not a scenario failure by itself unless --strict-visual, see baseline.rs docs) -- artifacts: {}",
                dir.display()
            );
        }
        Err(e) => {
            println!(
                "gold-journey[{checkpoint_key}]: WARNING -- baseline comparison failed to run: {e}"
            );
        }
    }

    Ok(())
}

fn write_artifacts(
    dir: &std::path::Path,
    viewport: &ViewportSpec,
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
    let _ = artifacts::write_artifact(dir, "server.log", server.request_log().join("\n"));
}

/// Waits for [`CheckpointSpec::expected_screen`] (published by the review
/// seam) and then for #168's screenshot-stability streak, exactly like
/// `cold_menu::wait_for_readiness` but keyed on the review seam's screen
/// marker instead of the boot DOM signal (`status.app_booted()` is also
/// required throughout, as a belt-and-suspenders check -- every screen this
/// scenario visits is reached only after the app has booted).
fn wait_for_readiness(
    checkpoint: &Checkpoint,
    spec: &CheckpointSpec,
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

        let ready_screen = status.app_booted() && screen.as_deref() == Some(spec.expected_screen);
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

    let reached_screen = last_screen.as_deref() == Some(spec.expected_screen);
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

fn check_required_assets(spec: &CheckpointSpec, status: &PageStatus, problems: &mut Vec<String>) {
    for (suffix, _source) in BASE_REQUIRED_ASSETS
        .iter()
        .chain(spec.extra_required_assets)
    {
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
            "document scrolls horizontally: scrollWidth {} > clientWidth {} (requested {})",
            status.scroll_width, status.client_width, viewport.width
        ));
    }
    if status.scroll_height > status.client_height + EPSILON {
        problems.push(format!(
            "document scrolls vertically: scrollHeight {} > clientHeight {} (requested {})",
            status.scroll_height, status.client_height, viewport.height
        ));
    }
    // #198: the requested device pixel ratio is per-viewport (1, 2, or 3),
    // not fixed at 1 -- see `browser::launch`'s doc comment for how the CDP
    // device-metrics override applies it.
    if (status.device_pixel_ratio - viewport.dpr).abs() > f64::EPSILON {
        problems.push(format!(
            "devicePixelRatio was {}, expected exactly {} (device-metrics override did not take)",
            status.device_pixel_ratio, viewport.dpr
        ));
    }
}

/// Pixel-level proof that *something* painted (not a blank/solid-color
/// canvas) and that the captured image is exactly the requested viewport
/// size scaled by the checkpoint's device pixel ratio (#198). Mirrors
/// `cold_menu::check_screenshot_pixels` (which stays fixed at DPR 1).
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
    // `Page.captureScreenshot`'s clip is specified in CSS pixels; the PNG
    // it returns is physical pixels, scaled by the tab's overridden device
    // pixel ratio -- so the expected image size is the logical viewport
    // times `dpr`, not the logical viewport itself (only true at DPR 1).
    let expected_width = (f64::from(viewport.width) * viewport.dpr).round() as u32;
    let expected_height = (f64::from(viewport.height) * viewport.dpr).round() as u32;
    if image.width() != expected_width || image.height() != expected_height {
        problems.push(format!(
            "screenshot was {}x{}, expected {}x{} ({}x{} logical viewport at {}x DPR)",
            image.width(),
            image.height(),
            expected_width,
            expected_height,
            viewport.width,
            viewport.height,
            viewport.dpr,
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
             the screen likely never painted (blank canvas)"
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

    // These tests exercise `resolve_viewports` -- the pure selection logic
    // -- directly, never the `XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX` env
    // var itself, so they can't race with anything else in the test binary
    // that touches the process environment (see `resolve_viewports`'s doc
    // comment for why).

    #[test]
    fn default_selection_is_desktop_dpr1_only() {
        let viewports = resolve_viewports(None);
        assert_eq!(viewports.len(), 1, "#284: default must be desktop-only");
        assert_eq!(viewports[0].name, "desktop");
        assert_eq!(viewports[0].width, 1280);
        assert_eq!(viewports[0].height, 800);
        assert_eq!(viewports[0].dpr, 1.0);
    }

    #[test]
    fn falsy_or_unrecognized_env_values_stay_narrowed() {
        for value in [None, Some("0"), Some("false"), Some(""), Some("nonsense")] {
            let viewports = resolve_viewports(value);
            assert_eq!(
                viewports.len(),
                1,
                "env value {value:?} should not activate the full matrix"
            );
            assert_eq!(viewports[0].name, "desktop");
        }
    }

    #[test]
    fn full_matrix_opt_in_restores_all_six_viewports() {
        for value in ["1", "true", "TRUE", "True"] {
            let viewports = resolve_viewports(Some(value));
            assert_eq!(
                viewports.len(),
                6,
                "env value {value:?} should activate the full #198 matrix"
            );
            let names: Vec<&str> = viewports.iter().map(|v| v.name).collect();
            assert_eq!(
                names,
                vec![
                    "desktop",
                    "desktop-dpr2",
                    "desktop-dpr3",
                    "phone",
                    "phone-dpr2",
                    "phone-dpr3",
                ]
            );
        }
    }

    #[test]
    fn full_matrix_dprs_match_the_documented_table() {
        let viewports = resolve_viewports(Some("1"));
        let dprs: Vec<f64> = viewports.iter().map(|v| v.dpr).collect();
        assert_eq!(dprs, vec![1.0, 2.0, 3.0, 1.0, 2.0, 3.0]);
    }

    /// Pins acceptance criterion "the current-screen gold journey remains
    /// covered": narrowing the viewport matrix must never touch which
    /// screens this scenario visits.
    #[test]
    fn all_five_current_screens_remain_checkpoints() {
        let names: Vec<&str> = CHECKPOINTS.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec!["menu", "creation", "fight", "fight-result", "shop"]
        );
    }
}
