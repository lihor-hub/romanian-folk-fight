//! `cargo xtask test <logic|journey>` -- focused Rust test entry points.
//!
//! Root-owned: this module covers only the fast pure-logic suite and one
//! existing headless multi-step journey test. It must not grow asset (#141)
//! or browser-smoke (#144) subcommands; those land as their own modules.

use std::process::Command;

use crate::process::{StepError, effective_budget_ms, run_step, warn_if_over_budget};

/// Target warm-run budget (milliseconds) shared by `test logic` and `test
/// journey`: both are the "focused pure/headless test" loop from the
/// player-experience rework plan's feedback-loop contract (30 seconds warm).
/// Overridable per-invocation via `XTASK_BUDGET_MS`; see
/// `docs/feedback-budgets.md` for the measured cold/warm timings this is
/// based on.
const FOCUSED_TEST_BUDGET_MS: u64 = 30_000;

pub const ABOUT: &str = "Focused Rust test suites (fast pure-logic units, one headless journey).";

pub const SUBCOMMANDS: &[(&str, &str)] = &[
    (
        "logic",
        "Fast pure-logic unit tests: character/stats, combat engine+ai, creation draft, items, progression level curve, roster ladder.",
    ),
    (
        "journey",
        "One existing headless multi-step GameState journey: Fight -> FightResult (payout/XP) -> Fight -> FightResult -> GameOver -> reset.",
    ),
];

pub fn run(sub: &str) -> Result<(), StepError> {
    match sub {
        "logic" => logic(),
        "journey" => journey(),
        other => unreachable!("dispatch validates subcommands before calling run; got {other}"),
    }
}

/// Pure game-rule modules with no Bevy `App`/ECS scaffolding: no plugin
/// setup, no `MinimalPlugins`, nothing render- or asset-adjacent. They're
/// the fastest meaningful subset of the suite and cover the formulas/state
/// machines (damage, hit/crit rolls, AI choice, character creation draft,
/// item catalog/equipment, level curve, ladder scaling) that the rest of the
/// game is built on -- a tight loop for iterating on game-rule changes
/// without paying for `cargo test`'s ~360 headless-Bevy-App tests.
///
/// Each entry is a `cargo test` name-substring filter; the test harness
/// treats multiple trailing filter args as an OR, so this runs the union.
const LOGIC_FILTERS: &[&str] = &[
    "character::",
    "combat::ai::",
    "combat::engine::",
    "creation::draft::",
    "items::",
    "progression::level::",
    "roster::",
];

fn logic() -> Result<(), StepError> {
    let mut cmd = Command::new("cargo");
    cmd.arg("test").arg("--lib").arg("--");
    cmd.args(LOGIC_FILTERS);
    let report = run_step("test logic", cmd)?;
    warn_if_over_budget(
        &report.label,
        report.elapsed,
        effective_budget_ms(FOCUSED_TEST_BUDGET_MS),
    );
    Ok(())
}

/// The closest existing headless "journey" test in the repo today. It drives
/// `progression`'s `GameState` machine through Fight -> FightResult (payout
/// and XP credited), then Fight -> FightResult again (payout accumulates),
/// then GameOver, then presses the game-over screen's real "back to menu"
/// button (`Interaction::Pressed` on the actual UI entity) and asserts the
/// run resets: lifetime earnings clear.
///
/// It does not yet cover MainMenu/CharacterCreation/Shop; the tracker
/// (#151/#142) plans a full menu -> creation -> fight -> result/shop -> next
/// fight -> defeat -> reset journey. That does not exist yet, so this
/// targets the closest true multi-step, headless, state-driven test until
/// #142's children add the complete flow.
const JOURNEY_TEST: &str =
    "progression::tests::lifetime_earnings_accumulate_across_wins_and_reset_with_the_run";

fn journey() -> Result<(), StepError> {
    let mut cmd = Command::new("cargo");
    cmd.arg("test").arg("--lib").arg("--");
    cmd.arg(JOURNEY_TEST).arg("--exact");
    let report = run_step("test journey", cmd)?;
    warn_if_over_budget(
        &report.label,
        report.elapsed,
        effective_budget_ms(FOCUSED_TEST_BUDGET_MS),
    );
    Ok(())
}
