# Contributing to Romanian Folk Fight

Thanks for considering a contribution. This is a small, hobby-scale project —
a browser-based turn-based arena RPG cast from Romanian folklore, built with
Rust and Bevy — but it takes real contributions seriously.

## What's wanted

- **Code** — gameplay systems, UI, combat balance, accessibility, CI/tooling.
- **Art and audio assets** — pixel art, sprites, sound effects, music.
- **Game design feedback** — pacing, difficulty curve, economy balance.
- **Playtesting** — try the [live build](https://lihor-hub.github.io/romanian-folk-fight/)
  and report what's confusing, broken, or un-fun.
- **Romanian folklore and language review** — the game draws on strigoi,
  vârcolaci, zmei, and other folk figures, plus Romanian vocabulary and
  diacritics throughout the UI. Corrections and cultural context are
  genuinely valuable, even without any code.
- **Docs** — this file, `README.md`, `xtask/README.md`, `docs/`.

If you're not sure whether an idea fits, open an issue and ask before
building something large.

## Dev environment setup

Prerequisites: [rustup](https://rustup.rs/) and, optionally but recommended,
[pre-commit](https://pre-commit.com/).

```bash
git clone https://github.com/lihor-hub/romanian-folk-fight.git
cd romanian-folk-fight
scripts/bootstrap-worktree.sh   # verifies cargo, wires the cargo xtask alias, installs git hooks
```

`scripts/bootstrap-worktree.sh` also installs the pre-commit hooks defined in
`.pre-commit-config.yaml`: generic hygiene plus `cargo fmt --all -- --check`
on commit, and one unconditional `cargo xtask pre-push` full gate on push.
This also covers non-Rust/config-only pushes. `pre-commit run --all-files`
runs commit-stage hygiene, not the push gate. To install both stages without
rerunning bootstrap, use
`pre-commit install --hook-type pre-commit --hook-type pre-push`.

### Native loop (fastest iteration)

```bash
cargo run --features dev
```

The `dev` feature turns on Bevy's dynamic linking for fast incremental
compiles. **Never** enable `dev` for `cargo build`/`--release` or for wasm
builds — it must not leak into anything that gets shipped.

### Browser loop

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk        # or: brew install trunk
trunk serve                # serves on http://localhost:8080
```

`trunk build --release` produces the distributable bundle in `dist/`.

## Verification before pushing

With both hooks installed, use normal `git commit` and `git push`. The push
hook is the authoritative local full gate: workspace fmt, default/review
workspace lint/tests, then the native/release/wasm check matrix. It stops on
failure and blocks the push. These are the native CI commands it covers:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features review -- -D warnings
cargo test --workspace
cargo test --workspace --features review
```

Both feature configurations stay in the local gate; neither is CI-only.
The additional check matrix stays free of `dev` and `review`. CI's separate
production wasm build, browser checks, and asset checks remain required.

Do not automatically run a manual full gate and then repeat it in the hook.
The hook's successful `cargo xtask pre-push` satisfies the before-push
requirement. If hooks are unavailable, run that command manually before
pushing; never bypass installed hooks. An explicit manual preflight is also
available when earlier full feedback is worth the extra run, but there is
no success receipt/cache: the push hook still reruns the full gate. Restage
and recommit any hook edits before retrying; each attempted push gets a fresh
full check of its current inputs.

Choose one relevant inner loop while editing (a focused test, or
`cargo xtask test logic` for broad pure-rule changes), rather than stacking
redundant suites. Observed local full-gate steps on the Linux worker took
12–14 minutes, plus roughly 6 minutes of Cargo/xtask launcher compilation in
several runs; timings vary by host and cache. These are planning observations,
not new budgets or a measured speedup.

Hook-policy regression tests: `python3 scripts/test-hooks.py` (requires
pre-commit and its PyYAML dependency). They run real commit/amend/push stages
in temporary repositories with a recording Cargo stub, covering config-only
pushes, failures/retry, and formatter edits. They do not build Rust or replace
the real canonical gate on your project push.

If your change touches anything under `assets/`, also run
`cargo xtask assets check` to validate manifests and credits. If it touches
rendered UI, `cargo xtask web-smoke --scenario gold-journey` (or a more
targeted scenario — see `xtask/README.md` for the full list) exercises a real
browser build; budget about 10 minutes from a cold wasm build.

See `xtask/README.md` for what each `cargo xtask` command does and
`docs/feedback-budgets.md` for measured timings.

## Project conventions

- Runtime code follows Bevy's ECS conventions (components, systems, states),
  organized as one plugin per feature under `src/` (see `src/lib.rs`'s
  `GamePlugin` for the full list — `combat`, `creation`, `shop`, `town`,
  `save`, etc.).
- Cargo features are explicit and opt-in — `dev` for native dynamic linking,
  `review` for the deterministic browser-smoke test seam — never enabled by
  default or leaked into a shipped build.
- If you're an AI coding agent working in this repo, read `AGENTS.md` first;
  it covers infrastructure knowledge, verification-gate costs, and workflow
  rules specific to agent contributions.

## Finding something to work on

Issues are labeled to help you find a fit:

- `good first issue` — a small, scoped starting point.
- `help wanted` — open for anyone, no special context needed.
- `ready-for-agent` — specified precisely enough for an AI agent to
  implement without further clarification.
- `ready-for-human` — needs human judgment, playtesting, or a design call
  that isn't fully nailed down yet.
- Subsystem labels — `gameplay`, `ui`, `assets`, `ecs`, `web`, `ci-cd` — tell
  you which part of the codebase an issue touches.

Comment on an issue to claim it before starting substantial work, so effort
doesn't collide with someone else already on it.

## PR guidelines

- Keep PRs small and focused on one change; split unrelated fixes into
  separate PRs.
- Title PRs in conventional-commit style, matching recent history, e.g.
  `fix: tighten button-color tolerance so the panel border stops reading as a button`
  or `feat: add the Town hub and route the core loop through it`. Look at
  `git log` for more examples of the house style.
- Link the issue the PR closes (e.g. `Closes #123`).
- Describe what verification you ran (which commands, and the result) —
  this is what reviewers and CI both check against.

## Asset contributions

Assets are licensed per-file, not under one blanket project license — see
[`assets/CREDITS.md`](assets/CREDITS.md) for the existing inventory and its
format. If you add a new asset:

- Add or update the corresponding `manifest.toml` sidecar (see
  `xtask/README.md`'s "assets check" section for the schema), and run
  `cargo xtask assets check` to validate it.
- Add a row to `assets/CREDITS.md` with the file's source and license,
  matching an existing row's format and wording.
- Include real provenance: where the asset came from, who made it, and
  under what license/terms it can be used here. Contributions without clear
  provenance can't be merged.

## Licensing

By contributing, you agree your contribution is dual-licensed under
[Apache-2.0](LICENSE-APACHE) and [MIT](LICENSE-MIT), the same as the rest of
the project, without any additional terms or conditions — the standard
Rust-ecosystem "inbound license = outbound license" convention.
