# Issue #312 report — save-reload Chrome/CDP death

## Outcome

The `save-reload` smoke harness now retains Chrome-owned crash/log evidence and
may replay its complete deterministic journey exactly once when, and only when,
`headless_chrome` reports the exact closed-connection signature observed in CI.
Product assertions, timeouts, console/page errors, malformed saves, and
near-match transport text remain fail-fast.

## Failure evidence and root cause

PR #330 workflow run `29705442299` failed on all three attempts. Artifacts
`8447806567`, `8447933158`, and `8448010532` each reached the same valid Shop
state and wrote `1-shop-before-reload.png`, then failed about three seconds
later with:

```text
waiting for an animation frame failed: Unable to make method calls because underlying connection is closed
```

All three `web-smoke-failure.log` files have SHA-256
`ee65d4256573e4bda9afde4d7f1cf9ff34b3a85127bf6c9180dc657ed6a7be69`.
They used Chrome `150.0.7871.114`; none of the old artifacts contained a Chrome
debug log or crash dump.

The repeatable evidence proves a dead CDP transport after the product reached a
valid persisted Shop checkpoint, not a failed game assertion. The missing
root-cause evidence came from the launcher's defaults: `headless_chrome` 1.0.22
adds `--disable-breakpad` and consumes Chrome's piped stderr only while finding
the DevTools URL. The old harness therefore recorded the client-side closed
connection but not Chrome's own exit/crash diagnostics.

## Implementation

- `browser::launch_with_diagnostics` keeps ordinary browser scenarios
  unchanged while opting `save-reload` into `--enable-logging`, an explicit
  `CHROME_LOG_FILE`, an explicit `BREAKPAD_DUMP_LOCATION`, and exclusion of the
  launcher's default `--disable-breakpad`.
- Each attempt keeps `chrome_debug.log` and `chrome-crashes/` beside its
  `chrome-profile`, where the workflow's existing scenario upload already
  collects them.
- Before a failed checkpoint drops the Chrome handle, the harness writes
  `chrome-process.log` with the observed error, Chrome PID, Linux
  `/proc/<pid>/{status,stat}` when available, and a portable `ps` snapshot.
  Diagnostic-write failure is reported separately and never replaces the
  original scenario failure.
- Attempt 1 remains `save-reload/journey`; a retry uses a fresh Chrome/profile
  at `save-reload/journey-retry`. Stale generated attempt directories are
  cleared before a new run, while both failed-attempt bundles survive during a
  retrying run.

## Exact retry contract

The classifier recognizes only the byte-exact, case-sensitive harness error:

```text
waiting for an animation frame failed: Unable to make method calls because underlying connection is closed
```

The full `wait_for_frame` wrapper is part of the match. This deliberately
rejects product/console assertions that merely quote the underlying library
phrase. The retry budget is one, yielding at most two complete browser
attempts. A second matching CDP death reports the first and final failures. Any
non-matching error on either attempt returns immediately, so an assertion after
a transport retry cannot trigger a third attempt.

## TDD and verification

RED/GREEN tests cover the exact observed signature, five non-matches, one
successful retry, retry-budget exhaustion after two deaths, an immediate
assertion failure, and an assertion after the sole transport retry. Browser
configuration tests cover attempt-local diagnostic paths, environment routing,
and removal of only `--disable-breakpad` from launcher defaults.

- RED classifier/retry tests: failed on the deliberately missing helpers.
- GREEN save-reload policy tests: 6/6 passed.
- RED browser diagnostic tests: failed on the deliberately missing diagnostic
  path/environment/default-argument helpers.
- GREEN browser tests: 5/5 passed.
- Review-fix RED: a console assertion containing the complete library phrase
  was initially misclassified by a substring match.
- Review-fix GREEN: the full observed harness error now matches by equality;
  the console assertion remains fail-fast.
- `cargo test -p xtask web_smoke::`: 62/62 passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy -p xtask --all-targets -- -D warnings`: passed.
- `NO_COLOR=true cargo xtask web-smoke --scenario save-reload`: passed locally
  with Chrome `147.0.7727.50`; all three journey screenshots were produced and
  reload/Continue restored Shop with wallet, ladder, and purchase intact.
- The successful local attempt produced `journey/chrome_debug.log` and
  `journey/chrome-crashes/settings.dat`; `journey-retry` was absent, proving a
  first-attempt pass does not run the retry.
- `cargo xtask pre-push`: all gates passed after the final rebase (fmt, default
  and review Clippy, default and review workspace tests, native debug, native
  release, and wasm build-matrix checks; 502.91 seconds).

## Independent review

Two independent agents reviewed commit `66b467a` against the repository
standards and issue #312 respectively. The standards reviewer found no defects
and independently passed 62 harness tests, xtask Clippy, formatting, and a
direct macOS `ps`-argument check. The spec reviewer found one important retry
boundary issue: the original substring classifier could retry a product/console
assertion containing the CDP phrase. The classifier was narrowed to byte-exact
equality and the new regression was observed RED then GREEN. No other spec gap
or unrelated scope change was found.

## Commits and PR state

- `4eda72e fix: harden save-reload CDP lifecycle (#312)`
- `9d87d2b test: pin exact CDP death boundary (#312)`
- `a6fbbce docs: record issue 312 verification`
- PR #331 is open and ready; squash auto-merge was enabled at
  `2026-07-19T23:47:46Z`, with required checks in progress. Its body contains
  `Closes #312` and ends with the required Antigravity trailer.

## Deviations

The implementation plan listed a single Cargo command with two test filters;
Cargo accepts only one positional filter, so the individual RED/GREEN commands
were run separately and the combined module suite was verified with
`cargo test -p xtask web_smoke::`. The desktop environment also exported
`NO_COLOR=1`, while Trunk 0.21.14 requires the environment-backed Boolean to be
`true` or `false`; the browser journey was therefore run with
`NO_COLOR=true`. The first full gate attempt also stopped during dependency
compilation with LLVM's `No space left on device`, after formatting and both
Clippy modes passed. The obsolete generated `target/` cache in PR #330's merged,
clean worktree was removed with `cargo clean`, reclaiming 20.1 GiB; source and
evidence were untouched. The complete gate then passed. None of these
environmental deviations changes production or CI behavior.

## PR #330 unblock

PR #330 merged as `2a566f0` before #312 was published, so it no longer needs a
rebase, rerun, or cherry-pick. This branch rebases onto that merge and changes
only the shared smoke harness for future runs; it does not modify #330's product
or UI work. Had #330 remained open, rebasing it after #312 landed would have
picked up the harness fix without a code cherry-pick, while rerunning its old
unrebased SHA would not have contained the fix.
