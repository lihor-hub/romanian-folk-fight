# Issue #248 delivery report

## Diagnosis

The issue originally described two 390x844 defects. Both production causes
were subsequently fixed without closing #248:

- #307 made phone rows flex the name cell while retaining the full 92px
  `Cumpără` / `Echipat` target.
- #308 made the phone body a non-wrapping, non-shrinking column whose resolved
  height includes the preview and catalog, so the following root-level
  `Înapoi în arenă` action clears the complete body.

The accepted post-#249 phone image already shows contained actions and no
preview overlap. #248 therefore needed issue-focused regression coverage and
fresh rendered proof on the final base, not another production layout change.

## Changes

- Added one 390x844 regression that exercises actual `Echipat` and `Cumpără`
  states, checks the row minimum against the clamped catalog, and ties the
  accepted vertical contract together: preview before catalog inside a plain
  non-shrinking column, with Back following that complete body at the root.
- Production layout, desktop appearance, #307's flexible phone rows, #249's
  compact non-9-slice chrome, and #120's standard-panel inset remain unchanged.

## TDD and focused tests

- The initial red test proposed moving Back inside the mobile body. It failed
  on the existing root-level relationship, but independent review correctly
  found that test was solution-shaped and conflicted with #308's accepted
  focus/layout order. The production change and exact-order assertion were
  removed.
- The corrected issue-focused regression preserves the accepted #307/#308
  contracts and passes without a production change. This is a deviation from
  red/green implementation because the reported behavior was already fixed on
  the synchronized base; fabricating a second layout change would be scope
  creep.
- `cargo test --lib shop::tests::` — 39 passed, 0 failed.
- Early full gate: `NO_COLOR=true cargo xtask pre-push` passed formatting,
  default/review Clippy, default/review tests, and native/release/WASM build
  matrix (939.29s from a cold worktree cache).
- Final post-#334-rebase `NO_COLOR=true cargo xtask pre-push` passed the same
  complete gate on the final implementation tree in 67.53s.

## Viewport and browser proof

- Waited for PR #334 to merge as `3a5e858`, fetched it, and rebased before
  capture. #334's two non-shop baselines and desktop fight-freeze paths remain
  untouched.
- `NO_COLOR=true XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX=1 cargo xtask
  web-smoke --scenario gold-journey` passed all 30 checkpoints (1209.04s):
  desktop and 390x844 phone at DPR 1, 2, and 3.
- All six shop cells matched their accepted baselines exactly, including
  `phone-shop`, `phone-dpr2-shop`, and `phone-dpr3-shop`. No baseline was
  rewritten or reaccepted because there was no intentional visual delta.
- Visually inspected the post-#334 desktop and 390x844 phone captures. Desktop
  retains the catalog-left/preview-right layout. On phone, `Echipat` and full
  `Cumpără` targets remain inside the gold row outline, and no Back action is
  painted across the preview frame; the root scroll owns access to the action
  below the complete preview-plus-catalog body.
- The scenario reported non-fatal animation-phase differences on unrelated
  creation/fight cells. They were neither used as #248 evidence nor accepted.

## Independent review

- Initial spec/standards review found the proposed mobile Back reorder was
  unnecessary scope creep, changed focus order, and encoded a solution rather
  than the already-fixed defect. The runtime edit and exact-order assertion
  were removed and this report's diagnosis was corrected.
- Both independent reviewers rechecked the revised diff and reported no
  remaining actionable spec or standards findings. They confirmed it preserves
  #307/#308, desktop behavior, #249 rows, #120, and the established focus order.

## Commits and PR state

- `355e6ac test: cover phone shop containment (#248)` adds the focused
  regression on the final base.
- `1350981 docs: record phone shop verification (#248)` records rendered,
  gate, and independent-review evidence.
- Ready PR [#335](https://github.com/lihor-hub/romanian-folk-fight/pull/335)
  closes #248 and carries the required Antigravity trailer. Squash auto-merge
  was enabled at 2026-07-20 09:23 UTC; required checks are running before the
  repository merge queue can admit it.

## Deviations and next notes

- No runtime layout, catalog content, character renderer, combat/progression
  logic, shared theme contract, unrelated screen, or xtask path is changed.
- PR #334 owned only the desktop fight freeze harness and two non-shop
  baselines. This branch rebased after it merged and does not touch those
  paths.
