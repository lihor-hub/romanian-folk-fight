# Issue #258 delivery report

## Diagnosis

The normal folk palette rendered `HP_BAR_ON_TRACK` with `HP_FILL`
(`srgb(0.78, 0.16, 0.14)`) over the unchanged carved-wood `BAR_TRACK`
(`srgb(0.16, 0.14, 0.13)`). Its WCAG 2.1 non-text contrast was below the
3:1 target. The high-contrast pair already met the target.

## Contrast ratios

| Palette | Before | After | Required |
| --- | ---: | ---: | ---: |
| Normal HP fill on track | 2.76:1 | 3.08:1 | 3.0:1 |
| High-contrast HP fill on track | 4.96:1 | 4.96:1 | 3.0:1 |

## Changes

- Retuned only the normal `HP_FILL` red component from 0.78 to 0.84.
- Made `accessibility_contrast` fail if normal `HP_BAR_ON_TRACK` falls below
  its non-text threshold; no waiver remains.
- Re-accepted only the direct desktop fight visual baseline.

## Evidence

- TDD red: focused contrast test failed with normal HP at 2.76:1.
- TDD green: `cargo test accessibility_contrast --lib -- --nocapture` reported
  normal HP at 3.08:1 and high-contrast HP at 4.96:1.
- Browser: `env -u NO_COLOR cargo xtask web-smoke --scenario
  fight-palette-desktop --update-baselines` was run from the rebased base and
  its screenshot was visually inspected. Telemetry reported seven buttons and
  that the palette fit inside the stage rect.
- Full gate: `cargo xtask pre-push` was run before publish. The local runner
  dropped its final stream after the observed fmt, clippy, default/review
  tests, and default/release/wasm checks; a fresh post-rebase gate is run
  before push.

## Review

An independent read-only review found no Critical or Important findings. Its
one Minor wording correction was applied before delivery.

## Delivery state

- Commit range: `origin/main..HEAD`.
- PR and merge-queue state: pending publication at the time this report was
  committed; see the final task delivery for the remote state.

## Deviations and next notes

- The environment exports `NO_COLOR=1`, which current Trunk passes to Cargo
  as an invalid `--no-color 1`; browser invocations used `env -u NO_COLOR`.
- Fresh-worktree builds exhausted the shared volume twice. Only this
  worktree's recoverable `target/` output was cleaned; shared caches and other
  worktrees were not modified by this task.
