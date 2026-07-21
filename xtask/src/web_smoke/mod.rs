//! Browser-smoke scenarios (#144/#168): build and serve the WASM game, drive
//! a real, freshly-launched browser against it, and verify a first-painted
//! screen. Entered from `cargo xtask web-smoke --scenario <name>
//! [--update-baselines]` via `crate::commands::web_smoke_cmd`, which is the
//! only file outside this directory that changes for this work (plus one
//! `GROUPS` line in `xtask/src/commands/mod.rs` -- see its module docs and
//! `xtask/README.md`'s extension pattern).
//!
//! ## Driver choice
//!
//! `headless_chrome` (CDP) driving a system Chrome/Chromium, not a
//! WebDriver client against `chromedriver`. See `browser`'s module docs for
//! the full rationale (no `chromedriver` needed/version-matched; forced
//! software rendering for cross-machine determinism).
//!
//! ## Server lifecycle
//!
//! [`build_release`] (shared by every scenario) runs `trunk build --release`
//! (through `crate::process::run_step`, so it gets the same artifact-log/
//! timing treatment as every other xtask step). `server::StaticServer` then
//! serves the resulting `dist/` on `127.0.0.1:<random free port>`
//! (`TcpListener` bound to port 0 -- no manual port picking) from its own
//! background thread until dropped, which happens automatically at the end
//! of a scenario's `run` (including on an early failure) since it's a plain
//! stack value with an RAII `Drop` impl. Deliberately not `trunk serve` --
//! see `server`'s module docs.
//!
//! ## Readiness contract
//!
//! See `cold_menu`'s module docs for the full per-frame (not time-based)
//! readiness contract: booted (loading screen gone, canvas present) then
//! stabilized (screenshot byte-identical across several consecutive
//! rendered frames), both bounded by a frame/wall-clock budget that fails
//! loudly (with artifacts) rather than silently passing.
//!
//! ## Baseline policy
//!
//! See `baseline`'s module docs: baselines live at
//! `tests/visual/baselines/<scenario>/<checkpoint>.png`, a normal run never
//! writes there, `--update-baselines` is the only thing that does, and a
//! missing/differing baseline is reported but does not by itself fail a
//! checkpoint (the explicit failure conditions are console/page errors,
//! missing required assets, and unexpected scroll/clipping -- see
//! `cold_menu`'s assertions).
//!
//! ## Artifact layout
//!
//! See `artifacts`'s module docs:
//! `target/xtask-artifacts/web-smoke/<scenario>/<checkpoint>/{screenshot.png,
//! console.log, network.log, viewport.log, server.log}`, written
//! unconditionally (pass or fail) so a failure's full diagnostics are always
//! on disk, with every path printed.
//!
//! ## The viewport/DPR matrix and `--all` (#198, narrowed by #284)
//!
//! `gold-journey`'s six screens (#129 added the town hub) can each run at
//! 1280x800 and 390x844, at device pixel ratios 1, 2, and 3 -- 36
//! checkpoints, driven by DPR-per-tab CDP emulation (see `browser::launch`'s
//! doc comment) rather than a new scenario or a second scenario runner. As
//! of #284, the *default* run narrows that to a representative viewport
//! (`normal-desktop`, 1440x900 @ DPR 1 -- 6 checkpoints); the full 30-checkpoint matrix is preserved and stays
//! reachable via an opt-in env var -- see `gold_journey`'s module docs'
//! "Desktop-only default scope (#284)" section for the selection mechanism
//! and reactivation path. `cargo xtask web-smoke --all` runs every
//! [`SCENARIOS`] entry (`cold-menu`, `gold-journey`,
//! `accessibility-settings-reload`, `reduced-motion-fight`,
//! `fight-palette-desktop`) in one invocation via [`run_all`], stopping at
//! the first failure like every other multi-step `xtask` command; the same
//! narrowed `gold-journey` default applies inside `--all` too.
//!
//! ## Visual-diff review gating (#198)
//!
//! `baseline`'s original policy is unchanged by default: a screenshot that
//! differs from its accepted baseline is reported but does not fail the
//! run. `--update-baselines` is still the only thing that writes a
//! baseline, and only for the scenario(s) actually run. `--strict-visual`
//! (or `XTASK_WEB_SMOKE_STRICT_VISUAL=1`) is a new, opt-in flag that turns
//! that same diff into an explicit checkpoint failure -- see
//! `commands::web_smoke_cmd` for where it's parsed and `baseline`'s module
//! docs for the full rationale. Whenever a diff exists (whether or not
//! `--strict-visual` is set), an `actual`/`expected`/`diff` PNG triplet is
//! written into that checkpoint's artifact directory
//! (`baseline::write_diff_triplet`) so CI can upload a focused, reviewable
//! bundle instead of requiring a manual diff against the committed
//! baseline. Baseline-free scenarios (`accessibility-settings-reload`,
//! `reduced-motion-fight` -- see their module docs) are unaffected by the
//! flag: they gate on their own exact assertions on every run.
//!
//! ## Adding a later scenario
//!
//! A later scenario adds its own module here (mirroring `cold_menu.rs`: a
//! `pub fn run(update_baselines: bool, strict_visual: bool) -> Result<(),
//! SmokeError>` -- scenarios without screenshot baselines may ignore both
//! flags, like `accessibility_settings_reload` does -- its own
//! `CheckpointSpec`s, its own assertions), one match arm in [`run_scenario`]
//! below, and one entry in [`SCENARIOS`] (so `--all` picks it up). Nothing
//! else changes: not the CLI surface (`--scenario <name>` already accepts
//! any name), not `commands::web_smoke_cmd`, not the dispatcher registration
//! in `commands::mod.rs`, and not the shared `browser`/`server`/`artifacts`/
//! `baseline` building blocks this module already provides.

mod abandon_forfeit;
mod accessibility_settings_reload;
pub mod artifacts;
pub mod baseline;
pub mod browser;
mod cold_menu;
mod corrupt_save_recovery;
mod desktop_fight_freeze;
pub mod error;
mod fight_palette_accessible;
mod fight_palette_desktop;
mod fight_palette_phone;
mod gold_journey;
mod high_contrast;
mod hybrid_2_5d_character;
mod keyboard_accessibility;
mod reduced_motion_fight;
mod romanian_paper_doll_library;
mod save_reload;
pub mod server;
mod touch_targets;
mod zoom_200;

use std::path::PathBuf;
use std::process::Command;

pub use error::SmokeError;

/// Every registered scenario, in the order `--all` (#198) runs them. A later
/// scenario module registers here (and in [`run_scenario`]'s match arm) --
/// nothing else about `--all`'s dispatch changes, per the extension pattern
/// this module's docs describe.
pub const SCENARIOS: &[&str] = &[
    "cold-menu",
    "gold-journey",
    "accessibility-settings-reload",
    "reduced-motion-fight",
    "fight-palette-desktop",
    "fight-palette-phone",
    "high-contrast",
    "fight-palette-accessible",
    "keyboard-accessibility",
    "zoom-200",
    "touch-targets",
    "corrupt-save-recovery",
    "save-reload",
    "abandon-forfeit",
    "hybrid-2-5d-character",
    "romanian-paper-doll-library",
];

/// Dispatches `--scenario <name>` to the matching scenario module. Known
/// scenarios: `cold-menu` (#168), `gold-journey` (#187/#198),
/// `accessibility-settings-reload` (#191), `reduced-motion-fight` (#200),
/// `fight-palette-desktop` (#189), `fight-palette-phone` (#199),
/// `high-contrast` (#214), `fight-palette-accessible` (#213),
/// `corrupt-save-recovery` (#201), and `save-reload`/`abandon-forfeit`
/// (#217) -- each one the exact extension pattern the module docs above
/// describe: a new module plus one match arm here, nothing else touched
/// upstream. `strict_visual` (#198) is forwarded to the scenarios with
/// screenshot baselines so a baseline diff can optionally fail the run
/// instead of the default non-fatal report -- see `baseline`'s module docs;
/// the baseline-free scenarios (including both #217 additions) have nothing
/// for it to gate.
pub fn run_scenario(
    scenario: &str,
    update_baselines: bool,
    strict_visual: bool,
) -> Result<(), SmokeError> {
    match scenario {
        "cold-menu" => cold_menu::run(update_baselines, strict_visual),
        "gold-journey" => gold_journey::run(update_baselines, strict_visual),
        "accessibility-settings-reload" => accessibility_settings_reload::run(update_baselines),
        "reduced-motion-fight" => reduced_motion_fight::run(update_baselines),
        "fight-palette-desktop" => fight_palette_desktop::run(update_baselines, strict_visual),
        "fight-palette-phone" => fight_palette_phone::run(update_baselines, strict_visual),
        "high-contrast" => high_contrast::run(update_baselines, strict_visual),
        "fight-palette-accessible" => {
            fight_palette_accessible::run(update_baselines, strict_visual)
        }
        "keyboard-accessibility" => keyboard_accessibility::run(update_baselines),
        "zoom-200" => zoom_200::run(update_baselines),
        "touch-targets" => touch_targets::run(update_baselines),
        "corrupt-save-recovery" => corrupt_save_recovery::run(update_baselines),
        "save-reload" => save_reload::run(update_baselines),
        "abandon-forfeit" => abandon_forfeit::run(update_baselines),
        "hybrid-2-5d-character" => hybrid_2_5d_character::run(update_baselines, strict_visual),
        "romanian-paper-doll-library" => {
            romanian_paper_doll_library::run(update_baselines, strict_visual)
        }
        other => Err(SmokeError::usage(format!(
            "unknown --scenario `{other}` (known scenarios: {})",
            SCENARIOS.join(", ")
        ))),
    }
}

/// `cargo xtask web-smoke --all` (#198): runs every [`SCENARIOS`] entry in
/// order, stopping at the first failure -- the same "stop at first failure"
/// convention every other multi-step `xtask` command uses (see
/// `xtask/README.md`'s "Process/result conventions"). `cold-menu` (2
/// checkpoints) plus `gold-journey`'s checkpoints (6 by default since #284's
/// desktop-only narrowing, or 36 with its full DPR matrix opted back in --
/// see that module's docs) plus `accessibility-settings-reload`'s,
/// `reduced-motion-fight`'s, and `fight-palette-desktop`'s checkpoints; a
/// later registered scenario adds its own on top without this function
/// changing.
pub fn run_all(update_baselines: bool, strict_visual: bool) -> Result<(), SmokeError> {
    for scenario in SCENARIOS {
        run_scenario(scenario, update_baselines, strict_visual)?;
    }
    Ok(())
}

/// The runtime-resolved workspace root used by every scenario to locate
/// `trunk build --release`'s working directory and the `dist/` it produces.
pub(crate) fn workspace_root() -> PathBuf {
    crate::process::workspace_root()
}

/// Runs `trunk build --release` (through `crate::process::run_step`, so it
/// gets the same artifact-log/timing treatment as every other xtask step)
/// and returns the `dist/` directory it produced. Shared by every scenario
/// that serves the plain release bundle (`cold_menu`,
/// `accessibility_settings_reload`); `gold_journey` deliberately keeps its
/// own `build_review_release` (a `--features review` build into its own
/// `dist-gold-journey/`) instead.
pub(crate) fn build_release(label: &str) -> Result<PathBuf, SmokeError> {
    let mut cmd = Command::new("trunk");
    cmd.arg("build").arg("--release");
    cmd.current_dir(workspace_root());
    // Deliberately not `--features dev`: that Cargo feature (Bevy dynamic
    // linking) exists only for fast native iteration and must never leak
    // into a release/wasm artifact (see `AGENTS.md`); `Trunk.toml` and this
    // invocation never pass it.
    crate::process::run_step(label, cmd)?;
    Ok(workspace_root().join("dist"))
}
