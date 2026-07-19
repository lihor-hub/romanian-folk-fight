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
- Full Rust suite: `cargo test` — pass (696 library tests, 0 failures).
- Formatting: `cargo fmt --all -- --check` — pass.
- Lint: `cargo clippy --all-targets -- -D warnings` — pass.
- Full gate and browser proof: pending the required base rebase after #312
  and #330 land.

## Viewport evidence / browser

- Desktop evidence: 1440x900 normal desktop shop checkpoint, pending the
  required rebase after queued PRs #329 and #330 land.
- Phone evidence: 390x844 shop checkpoint through the same gold-journey
  scenario, pending the same rebase. The phone row button constraints are
  unchanged; no claim about #248 is made until the browser evidence exists.

## Review

Independent read-only review found no critical or important code defects.
It confirmed the scope stays local to shop rows and requested rebase plus
browser proof before merge. It also noted the theme module's generic
`panel_bundle` documentation still names shop row groups; that wording is
left untouched because #249 explicitly excludes shared theme changes.

## Commits and PR

Commit: `0c929f5 fix: keep shop row content clear of embroidered border`.

PR: pending; it will close #249 and include the required Antigravity trailer,
then enter the squash merge queue.

## Deviations and next notes

- No global theme, assets, renderer, combat/progression rules, result layout,
  or xtask harness changes are included.
- The compact-row change is intended to resolve #234 too, contingent on the
  1280x800 and 390x844 visual proof. #248 remains open unless the focused
  phone browser proof demonstrates all of its acceptance criteria.
