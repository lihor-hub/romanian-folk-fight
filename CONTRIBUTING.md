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
`.pre-commit-config.yaml` (`cargo fmt`/`cargo clippy` on commit, `cargo test`
on push, plus generic hygiene checks). Run `pre-commit run --all-files` any
time to check everything by hand.

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

CI runs these on every PR; run them locally first so you're not waiting on
CI to find a problem:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo xtask pre-push` runs the equivalent full gate in one step
(fmt, clippy, test — each run twice, once with default features and once
with `--features review` so `src/review/mod.rs` stays covered — plus the
native/release/wasm build matrix). Budget roughly 4 minutes for it; it's
required before every push, and CI enforces the same checks it runs.

While iterating on pure game-rule changes, `cargo xtask test logic` is a much
faster inner loop (runs only the modules that don't spin up a Bevy `App`).
Save the full `pre-push` gate for before you actually push.

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
