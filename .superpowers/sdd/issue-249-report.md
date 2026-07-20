# Issue #249 report — compact shop catalog rows

## Diagnosis

Each catalog item row used `theme::panel_bundle`, which correctly enforces
the #120 24px content inset for the embroidered 9-slice border. A row also
contains a 44px button, so its valid natural height is at least 92px. The
fixed-height, scrollable catalog flex-shrank those rows below that height,
painting the horizontal embroidery bands through item names and stats.

Using the standard 9-slice row at its required height would avoid the
overlap but would cause the oversized compact-row regression documented by
#234. The shared theme inset remains unchanged.

## Changes

- Replaced the shop-only catalog row's 9-slice chrome with compact walnut
  chrome and a 1px gold outline.
- Kept the 8px horizontal / 4px vertical row content inset, enforced a 52px
  minimum row height around the 44px tap target, and disabled row shrink so
  the catalog scrolls instead of compressing its content.
- Preserved the existing responsive name-cell flex and fixed trailing button
  constraints from #307.

## TDD and tests

- Regression test: `catalog_rows_use_compact_chrome_that_cannot_shrink_into_text`.
- Focused regression: `cargo test --lib
  shop::tests::catalog_rows_use_compact_chrome_that_cannot_shrink_into_text`
  — pass (1 test).
- Shop module: `cargo test --lib shop::tests` — pass (38 tests).
- Full Rust suite before the dependency rebase: `cargo test` — pass (696
  library tests, 0 failures).
- Formatting: `cargo fmt --all -- --check` — pass.
- Lint: `cargo clippy --all-targets -- -D warnings` — pass.
- Post-rebase `cargo xtask pre-push` — pass: fmt, default/review clippy,
  default/review tests, and native/release/wasm build matrix (399.38s).

## Viewport evidence / browser

- `env -u NO_COLOR XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX=1 cargo xtask
  web-smoke --scenario gold-journey --update-baselines` — pass, 30/30
  checkpoints across desktop and phone at DPR 1, 2, and 3.
- Visually inspected and retained only the six shop baselines. Item names,
  stats, prices, and buttons remain clear of row borders at every DPR.
- The 1280x800 desktop and 390x844 phone evidence confirms #234's compact-row
  acceptance criteria, so #234 is resolved by this change as well.
- #248 is not claimed: this journey proves the captured phone shop state but
  does not exercise every state in #248's acceptance criteria.

## Review

Independent read-only review found no critical or important code defects.
It confirmed the scope stays local to shop rows and requested rebase plus
browser proof before merge. It also noted the theme module's generic
`panel_bundle` documentation still names shop row groups; that wording is
left untouched because #249 explicitly excludes shared theme changes.
The final post-baseline review likewise found no critical or important
findings and confirmed the six retained baselines are shop-only.

## Commits and PR

Commits:

- `d663eb2 fix: keep shop row content clear of embroidered border`
- `75a97f6 test: accept compact shop row baselines`

PR: pending; it will close #249 and include the required Antigravity trailer,
then enter the squash merge queue.

## Deviations and next notes

- No global theme, assets, renderer, combat/progression rules, result layout,
  or xtask harness changes are included.
- The first browser launcher invocation stopped before building because the
  inherited `NO_COLOR=1` is not a boolean accepted by Trunk 0.21.14. The
  command above removed that variable and completed the same scenario.
- #248 remains open; no acceptance claim is made for it.
