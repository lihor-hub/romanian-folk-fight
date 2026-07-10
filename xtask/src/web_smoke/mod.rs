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
//! `cold_menu::build_release` runs `trunk build --release` (through
//! `crate::process::run_step`, so it gets the same artifact-log/timing
//! treatment as every other xtask step). `server::StaticServer` then serves
//! the resulting `dist/` on `127.0.0.1:<random free port>` (`TcpListener`
//! bound to port 0 -- no manual port picking) from its own background
//! thread until dropped, which happens automatically at the end of
//! `cold_menu::run` (including on an early failure) since it's a plain
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
//! ## Adding a later scenario
//!
//! A later scenario (e.g. a character-creation or in-fight smoke) adds its
//! own module here (mirroring `cold_menu.rs`: a `pub fn run(update_baselines:
//! bool) -> Result<(), SmokeError>`, its own `CheckpointSpec`s, its own
//! assertions) and one match arm in [`run_scenario`] below. Nothing else
//! changes: not the CLI surface (`--scenario <name>` already accepts any
//! name), not `commands::web_smoke_cmd`, not the dispatcher registration in
//! `commands::mod.rs`, and not the shared `browser`/`server`/`artifacts`/
//! `baseline` building blocks this module already provides.

pub mod artifacts;
pub mod baseline;
pub mod browser;
mod cold_menu;
pub mod error;
mod gold_journey;
pub mod server;

pub use error::SmokeError;

/// Dispatches `--scenario <name>` to the matching scenario module:
/// `cold-menu` (#168) and `gold-journey` (#187, the exact extension pattern
/// the module docs above describe -- a new module plus this one match arm).
pub fn run_scenario(scenario: &str, update_baselines: bool) -> Result<(), SmokeError> {
    match scenario {
        "cold-menu" => cold_menu::run(update_baselines),
        "gold-journey" => gold_journey::run(update_baselines),
        other => Err(SmokeError::usage(format!(
            "unknown --scenario `{other}` (known scenarios: cold-menu, gold-journey)"
        ))),
    }
}
