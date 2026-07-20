# Issue #316 delivery report

## Recovery state

- Resumed the preserved worktree at
  `/Users/ioachimlihor/.codex/worktrees/issue-316-visual-freeze/romanian-folk-fight`
  on `codex/fix-desktop-fight-freeze-316`; no replacement worktree was made.
- The preserved branch was clean, had no branch-only partial implementation,
  and did not yet contain this report.
- Rebased from `c6c0292` onto `origin/main` at `e686830` before editing or
  accepting visual baselines.
- #304, #312, and #250 were already present on the synchronized base. After
  implementation and review, rebased again onto `origin/main` at `5082885`
  after #333 merged. #333's shop work and every shop baseline remain
  untouched.

## Scope and diagnosis

Owned cells:

- `high-contrast/desktop-fight`
- `fight-palette-accessible/desktop`

The desktop harness paused `Time<Virtual>` only after Fight had been reached,
so the captured phase depended on wall-clock boot/navigation time. The first
fix attempt proved that setting an absolute elapsed value after Fight spawned
was still incomplete: fighter-local animation timers could consume the large
elapsed jump from different starting states even while root/parallax motion
telemetry held still.

The shared helper now performs the complete deterministic order:

1. pause `Time<Virtual>`;
2. set its absolute elapsed target to 10,000 seconds;
3. enter Fight while the clock is already frozen;
4. read two consecutive `rff_review_motion_v1` snapshots and require exact
   equality before capture.

Only the two desktop paths use the helper. High-contrast menu/phone,
fight-palette-accessible phone, the standalone palette scenarios, and shop
paths retain their previous behavior.

## Deterministic proof

Before the fix on rebased `e686830`:

| Cell | Non-strict diff |
| --- | ---: |
| `high-contrast/desktop-fight` | 71,449 / 1,024,000 px |
| `fight-palette-accessible/desktop` | 45,243 / 1,024,000 px |

The desktop menu control matched at 0 px. Phone cells were observed but were
neither used as acceptance evidence nor rewritten.

After the final freeze-before-entry ordering:

| Cell | Repeated accepted SHA-256 | Strict result |
| --- | --- | --- |
| `high-contrast/desktop-fight` | `6e17a1ccaa9ce23b167a22f4888d7328564282d5135816e8811dda755ef6d317` | 0 px / byte match |
| `fight-palette-accessible/desktop` | `4fc41e93d6b538a1c338291b5c6bca802f86c6929bbfe997aa666c9c79cb9aa6` | 0 px / byte match |

Both actual/diff pairs were visually inspected before acceptance. Baselines
were accepted through the repository command with a temporary desktop-only
viewport selection; that selection was restored immediately. Git confirms
that the only baseline changes are the two owned desktop PNGs. After rebasing
over #333, both owned desktop cells were run strict and serialized once more;
each remained an exact accepted-baseline match, and the temporary viewport
selection was again restored.

## TDD and verification

- RED: `cargo test -p xtask desktop_fight_freeze -- --nocapture` failed with
  the legacy pause-only command plan.
- GREEN: the focused helper suite passed 4/4 tests.
- Second RED: `cargo test -p xtask freezes_the_clock_before_entering_the_fight -- --nocapture`
  failed on `EnterFight -> Pause -> SetElapsed -> AssertMotion`.
- Second GREEN: the focused helper suite passed 5/5 tests with
  `Pause -> SetElapsed -> EnterFight -> AssertMotion`.
- Serialized full scenario runs passed:
  `NO_COLOR=true cargo xtask web-smoke --scenario high-contrast` and
  `NO_COLOR=true cargo xtask web-smoke --scenario fight-palette-accessible`.
- Serialized desktop-only strict repeats passed both owned cells at 0 px;
  the full viewport lists were restored afterward.
- `CARGO_INCREMENTAL=0 NO_COLOR=true cargo xtask pre-push`: passed all gates
  (fmt, default/review Clippy, default/review tests, and native/release/wasm
  no-dev build matrix). Two earlier attempts stopped during the cold default
  test compile only because the shared volume was full; after removing
  recoverable generated caches and restoring disk capacity, the complete gate
  passed in 876.37 seconds.
- After fixing the independent review finding, the complete same command
  passed again from warm caches in 22.66 seconds.
- After the final rebase onto `5082885`, the complete same command passed on
  the final implementation tree in 114.29 seconds. The repository-enforced
  pre-push hook also passed when publishing the branch.

`NO_COLOR=true` was supplied because the host exports `NO_COLOR=1`, while the
installed Trunk accepts only the boolean spellings `true` and `false`.

## Review, commits, and PR

- Independent standards review: no hard violations or judgment-call findings.
- Independent spec review: one P2 found that the typed motion mirror ignored
  `generated_opponent`, so equality was not complete. Fixed by preserving all
  additional telemetry fields via `serde(flatten)` and adding a RED/GREEN
  regression where only that field changes; the helper suite now passes 6/6.
- Implementation commit: `18cf687 test: freeze desktop fight visual phases
  (#316)`, based on `5082885`.
- Ready PR: [#334](https://github.com/lihor-hub/romanian-folk-fight/pull/334),
  which closes #316 and ends with the Antigravity trailer. CI started after
  publication; squash auto-merge will be queued after this final report
  update is published.
