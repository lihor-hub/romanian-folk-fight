# Issue #250 — result panel content-box repair

## Diagnosis

The fight-result panel relied on intrinsic, shrinkable flex sizing. Its fixed
300px XP bar therefore determined the panel's outer width rather than living
inside the panel's existing 28px border inset. The panel could also shrink
vertically inside the scrollable result-screen shell, allowing its fixed-height
action buttons to meet or overhang the bottom border.

## Changes

- Made the result panel an explicit `ContentBox` with a 300px content width.
- Sized the XP track to 244px, explicitly reserving the visible 28px inset on
  each side in Bevy's rendered wasm layout.
- Disabled flex shrinking on that panel so Bevy cannot collapse the reserved
  inset space back down to the XP bar's intrinsic width.
- Grouped the fixed-height action buttons and gave their group an explicit
  28px bottom margin. This works around Bevy's intrinsic-height calculation,
  which did not reserve the panel's bottom padding and let the last action
  cross the embroidery.
- Added a focused regression test pinning the panel width, XP width, and action
  group bottom-inset constraints.

## TDD and focused verification

Red:

```text
result_panel_keeps_xp_and_actions_inside_its_content_box
left: Auto
right: Percent(100.0)
```

Subsequent red/green steps caught details visible only after rendering: Bevy's
intrinsic flex sizing did not reserve parent padding for the fixed child, and
percentage/fixed border-box attempts remained ambiguous. The final focused
test therefore requires an explicit 300px `ContentBox`, a non-shrinking panel,
the derived 244px XP width, and the explicit action-group inset.

Green:

```text
cargo test progression::result_ui::tests::result_panel_keeps_xp_and_actions_inside_its_content_box -- --exact
1 passed

cargo test progression::result_ui::tests
13 passed

cargo fmt --all -- --check
PASS

cargo clippy --all-targets -- -D warnings
PASS
```

## Browser verification

PR #330 merged as `2a566f0`; this branch was rebased onto that commit before
capturing baselines. The final layout passed the required desktop and phone
DPR 1/2/3 matrix:

```text
NO_COLOR=true XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX=1 \
  cargo xtask web-smoke --scenario gold-journey \
  --update-baselines --strict-visual
PASS — 30/30 checkpoints
```

Only the six fight-result baselines were retained after the final recapture.

There were two unrelated transient harness retries: one run stopped without a
detail at `desktop-dpr3-menu`, and another hit lazy-loading failures for the
required torso/feet icon assets on `desktop-shop`. The unchanged final layout
then completed the entire 30-checkpoint matrix successfully.

## Delivery state

- `cargo xtask pre-push`: PASS (331.67s; fmt, default/review clippy and tests,
  native/release/wasm build matrix)
- Independent standards/spec review: final re-review found no actionable
  findings after the visibly inset desktop/phone capture
- Commit/PR/merge queue: branch ready; PR creation and squash-queueing follow
  this report commit

## Scope and deviations

Only `src/progression/result_ui.rs`, this required report, and the six directly
affected fight-result baselines are modified. Baseline captures for unrelated
screens were restored. No shared theme token, combat rule, renderer,
shop/creation layout, xtask harness, or background-freeze behavior was changed.

The first pre-push attempt after final rendering exhausted the volume while
writing a Bevy `.rmeta`. Cleaning this worktree's recoverable Cargo cache freed
22 GiB; the complete cold-cache rerun then passed.
