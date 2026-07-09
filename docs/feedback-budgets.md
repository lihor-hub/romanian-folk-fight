# Worktree feedback budgets

Owned by [#165](https://github.com/lihor-hub/romanian-folk-fight/issues/165), a
leaf of [#152](https://github.com/lihor-hub/romanian-folk-fight/issues/152).
Records representative cold/warm timings, the target budgets `xtask` warns
against, and the methodology behind both, for the four root-owned commands:

```
cargo xtask test logic
cargo xtask test journey
cargo xtask check build-matrix
cargo xtask pre-push
```

See `xtask/README.md` for what each command actually runs and the
process/artifact conventions they share. This document only covers timing.

## Representative machine

All measurements below were taken on one machine, once, as a representative
sample -- not a statistically rigorous benchmark. Re-measure if the toolchain,
hardware, or cache setup changes materially.

| Field | Value |
| --- | --- |
| CPU | Apple M5 Pro (`sysctl -n machdep.cpu.brand_string`) |
| Cores | 18 (`sysctl -n hw.ncpu`) |
| RAM | 24 GiB (`sysctl -n hw.memsize` = 25769803776 bytes) |
| OS | macOS 26.5.1, build 25F80 (`sw_vers`) |
| Rust | `rustc 1.96.1 (31fca3adb 2026-06-26)` |
| Cargo | `cargo 1.96.1 (356927216 2026-06-26)` |
| Compiler cache | **Disabled** -- `sccache` not found on `PATH`. `scripts/bootstrap-worktree.sh` printed: `Shared compiler cache: DISABLED (sccache not found on PATH).` All timings below are therefore *without* a warm sccache; installing it (`brew install sccache`) and re-running bootstrap would very likely lower the cold numbers, especially for the build-matrix step where the same dependency graph is compiled three times (native/release/wasm). |
| Worktree | Fresh git worktree for this issue; `target/` did not exist before the first command ran. |

## Methodology

- **Cold** = immediately after `cargo clean` (verified: the worktree's very
  first command, `cargo xtask test logic`, ran with no `target/` directory at
  all, so its measurement is a genuine first-compile cold start; every other
  command's cold number below was captured by running `cargo clean` first,
  since by that point the shared `target/` already held artifacts from the
  prior command).
- **Warm** = the same command run again immediately afterward, twice in a
  row, with no source changes in between. Both warm numbers are reported to
  show run-to-run variance.
- Each command's own internal timing (the `elapsed` field on `xtask`'s
  `StepReport`, printed as `ok (N.NNs)` in its transcript) is what's recorded
  -- not wall-clock `time` around the whole `cargo xtask` process (which adds
  a negligible ~0.1-0.5s for compiling/launching the `xtask` binary itself).
- All numbers are single samples on one machine, not averages over many
  runs -- they establish a representative order of magnitude and confirm the
  budget-warning mechanism, not a precise SLA.

## Cold / warm timing table

| Command | Cold | Warm (run 1) | Warm (run 2) | Target budget (warm) |
| --- | ---: | ---: | ---: | ---: |
| `test logic` | 188.29s | 0.37s | 0.36s | 30s |
| `test journey` | 362.63s | 0.53s | 0.30s | 30s |
| `check build-matrix` | 229.43s (80.90 + 58.29 + 90.24) | 0.74s (0.32 + 0.23 + 0.20) | 0.45s (0.16 + 0.15 + 0.14) | 240s |
| `pre-push` | 480.97s (fmt 0.19 + clippy 58.40 + test 206.58 + check 2.75/57.15/155.91) | 2.11s | 1.72s | 600s (10 minutes) |

`check build-matrix`'s three `cargo check` invocations (native, release,
wasm) each get their own row in `xtask`'s per-step transcript; the cold/warm
figures above are their sum, matching the budget check in
`xtask/src/commands/check_cmd.rs`, which compares the *summed* elapsed time of
all three steps against one target, not each individually.

### Observed variance

`test journey`'s cold run (362.63s) took roughly **2x longer** than `test
logic`'s cold run (188.29s), even though both compile essentially the same
`--lib` test binary from the same dependency graph -- the only difference is
which test-name filter is passed to the already-compiled test binary, which
does not change what gets compiled. This is very likely machine-level
variance between two independent `cargo clean` + full-rebuild cycles run
back-to-back on this laptop (thermal throttling recovery, background
processes, filesystem cache eviction from the intervening `cargo clean`) --
not a real difference between the two commands. Anyone re-measuring should
expect similar order-of-magnitude cold numbers (a few minutes) but should not
be surprised if the two focused-test commands don't land within a tight
percentage of each other on a single sample.

Both `test logic` and `test journey` warm runs (bare seconds apart, both
under half a second) were consistent within run-to-run noise, well inside
their 30-second target -- once the workspace has been compiled once, the
inner loop this budget protects is effectively instantaneous.

`check build-matrix`'s warm runs (0.74s, then 0.45s) are also both essentially
no-op cache hits: with no source changes, all three `cargo check` invocations
just confirm already-fresh fingerprints. This is the best case, not a
worst-case "touch one file, re-run all three profiles" warm measurement --
that scenario was not captured here (see Known limitations).

`pre-push`'s cold run (480.97s summed, ~8m02s wall clock) sits inside its
10-minute budget even fully cold with no compiler cache. One nuance inside
that single cold run: its `check native` step took only 2.75s because the
`cargo test --workspace` step earlier *in the same run* had already compiled
the dev-profile dependency graph -- only the release (57.15s) and wasm
(155.91s) checks were still genuinely cold by the time the matrix ran. Warm
`pre-push` (2.11s, then 1.72s) still genuinely re-runs the full test suite
(379 game tests + 15 xtask tests, confirmed in
`target/xtask-artifacts/cargo-test.log`); the suite itself executes in well
under a second once compiled.

## Target budgets and where they come from

The [player-experience rework
plan](superpowers/plans/2026-07-09-player-experience-rework.md)'s "Feedback
loop contract" section names warm-run target budgets for the pure/headless
test loop (30s) and the full pre-push gate (10 minutes) directly; it does not
name one for `check build-matrix` (its budget table only covers focused
test/asset/gallery/browser-scenario/pre-push loops). This issue's choices:

- **`test logic` / `test journey`: 30 seconds.** Directly from the plan's
  "Focused pure/headless test" row -- both commands are exactly that class of
  loop (a `cargo test --lib` filter with no Bevy `App`/`MinimalPlugins`
  bootstrapping cost beyond what the crate always pays). Observed warm: well
  under 1 second, comfortably inside budget.
- **`check build-matrix`: 240 seconds.** Not named by the plan. Set as this
  issue's own initial target, with headroom over both extremes actually
  measured (a nearly-instant no-op warm rerun, and a ~230-second pure-cold
  rebuild of all three profiles) to approximate what a realistic "touch one
  file, re-check all three profiles" warm iteration might cost -- that
  specific scenario wasn't measured (see Known limitations), so this number is
  explicitly a placeholder to revise once it is.
- **`pre-push`: 600 seconds (10 minutes).** Directly from the plan's "Full
  pre-push gate" row.

Every budget is overridable per-invocation via the `XTASK_BUDGET_MS`
environment variable (milliseconds), documented in
`xtask/src/process.rs::effective_budget_ms`. This exists so the
budget-warning path can be exercised deterministically without hard-coding an
artificially tiny default into the real target -- e.g.:

```
XTASK_BUDGET_MS=1 cargo xtask test logic
```

forces the 1ms budget to be exceeded by any real test run, regardless of the
built-in 30-second default, without touching any other command's budget.

## Budget-warning transcript (test-only 1ms budget)

```
$ XTASK_BUDGET_MS=1 cargo xtask test logic

==> test logic
    $ cargo test --lib -- character:: combat::ai:: combat::engine:: creation::draft:: items:: progression::level:: roster::
    ok (0.31s) -- log: <worktree>/target/xtask-artifacts/test-logic.log
    WARNING: test logic took 0.31s, over its 0.00s target budget (see docs/feedback-budgets.md). The command's pass/fail result is unaffected.
$ echo $?
0
```

(The 1ms override renders as `0.00s` in the two-decimal seconds format --
warm real budgets are all whole seconds, so the display precision only looks
degenerate for this deliberately absurd test-only value.)

The command's own exit code is still `0` (success) -- the warning is printed
to stdout alongside the normal `ok (...)` line and never converts a passing
run into a failure. The same mechanism fired unprompted (no override needed)
on both of this issue's real cold runs, since 188.29s and 362.63s both
legitimately exceed the 30-second default budget:

```
    ok (188.29s) -- log: <worktree>/target/xtask-artifacts/test-logic.log
    WARNING: test logic took 188.29s, over its 30.00s target budget (see docs/feedback-budgets.md). The command's pass/fail result is unaffected.
```

## How the warning mechanism works

Implemented additively in `xtask/src/process.rs`, alongside the existing
`run_step`/`StepReport` conventions (no changes to their signatures):

- `effective_budget_ms(default_ms) -> u64` resolves a command's budget,
  letting `XTASK_BUDGET_MS` override the built-in default.
- `warn_if_over_budget(label, elapsed, budget_ms)` prints a `WARNING` line
  when `elapsed` exceeds `budget_ms`; otherwise it prints nothing. It is
  called *after* a step (or, for multi-step commands, the summed total of all
  their reports via `total_elapsed`) has already succeeded, so a slow run
  still returns its real success -- a budget overrun can never turn a passing
  `cargo xtask` invocation into a failing one, and a genuine failure (e.g. a
  broken test) still reports `StepError::Failed` exactly as before, untouched
  by any of this.

Each command module (`test_cmd.rs`, `check_cmd.rs`, `pre_push.rs`) declares
its own budget constant and calls these two functions after running; no
existing command's control flow, output on failure, or artifact retention
changed.

## Downstream ownership: asset and browser loops

The plan's feedback-loop contract also names two other loops this issue does
**not** implement or time -- they are separately owned:

- `cargo xtask assets check` / `cargo xtask assets review --changed` --
  [#141](https://github.com/lihor-hub/romanian-folk-fight/issues/141)'s asset
  manifest/gallery module (tracked leaves
  [#167](https://github.com/lihor-hub/romanian-folk-fight/issues/167)/[#185](https://github.com/lihor-hub/romanian-folk-fight/issues/185)/[#197](https://github.com/lihor-hub/romanian-folk-fight/issues/197)/[#211](https://github.com/lihor-hub/romanian-folk-fight/issues/211)).
  Target warm budgets from the plan: asset contract 5s, changed-asset gallery
  30s.
- `cargo xtask web-smoke --scenario <scenario-name>` --
  [#144](https://github.com/lihor-hub/romanian-folk-fight/issues/144)'s
  browser-smoke orchestration module (tracked leaves
  [#168](https://github.com/lihor-hub/romanian-folk-fight/issues/168)/[#187](https://github.com/lihor-hub/romanian-folk-fight/issues/187)/[#198](https://github.com/lihor-hub/romanian-folk-fight/issues/198)).
  Target warm budget from the plan: one browser scenario, 90s.

These two command names are documented here **only as names and their
already-planned target budgets** -- neither is implemented, wired into the
`xtask` dispatcher, nor measured by this issue. `xtask/src/commands/mod.rs`'s
own test (`help_lists_only_root_owned_commands`) asserts `cargo xtask --help`
never mentions "asset" or "browser" until #141/#144 add those modules
themselves. When they do, they should replace the two bullets above with
their own measured cold/warm tables, following this document's format.

## Known limitations

- All numbers are a single sample on one representative machine with no
  `sccache`, not a statistical distribution across hardware/cache
  configurations. Re-measure if either changes materially.
- `check build-matrix`'s warm figures are both no-op reruns with zero source
  changes; the more realistic "one file changed, re-check all three
  profiles" warm scenario was not captured, and `check build-matrix`'s 240s
  budget is this issue's own placeholder pending that data (see above).
- Two consecutive cold runs of structurally similar commands (`test logic`
  vs. `test journey`) differed by roughly 2x on this machine; treat the
  absolute cold numbers as order-of-magnitude, not precise.
