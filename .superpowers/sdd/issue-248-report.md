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
  matrix (939.29s from a cold worktree cache). A final post-rebase run remains
  pending.

## Viewport and browser proof

Pending PR #334 merge/rebase and serialized browser capture.

## Independent review

- Initial spec/standards review found the proposed mobile Back reorder was
  unnecessary scope creep, changed focus order, and encoded a solution rather
  than the already-fixed defect. The runtime edit and exact-order assertion
  were removed and this report's diagnosis was corrected.
- Both independent reviewers rechecked the revised diff and reported no
  remaining actionable spec or standards findings. They confirmed it preserves
  #307/#308, desktop behavior, #249 rows, #120, and the established focus order.

## Commits and PR state

Pending final verification and publication.

## Deviations and next notes

- No runtime layout, catalog content, character renderer, combat/progression
  logic, shared theme contract, unrelated screen, or xtask path is changed.
- PR #334 owns only the desktop fight freeze harness and two non-shop
  baselines. This branch will rebase after it merges and will not touch its
  paths.
