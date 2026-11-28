---
name: rust-dev
description: >-
  Rust and Bevy development standards and verification gate for this repo. Use when editing .rs
  files, Cargo dependencies, unit tests, or investigating clippy, fmt, or build failures.
---

# Rust and Bevy development in this repo

The game logic resides in `src/`, with assets (if any) in `assets/`.
Write code that clears the configured gates without loosening them.

## 0. Detect before you act

Use the Cargo commands:
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all -- --check`
- Test: `cargo test --workspace`
- Build/Check: `cargo check`

In a fresh worktree, run `scripts/bootstrap-worktree.sh` to ensure paths and hooks are configured. Bootstrap also wires the `cargo xtask` alias — until it has run, use `cargo run --package xtask -- <args>`.

While iterating on pure game-rule changes, `cargo xtask test logic` is the fast loop (skips the ~250 tests that boot a headless Bevy App). See `xtask/README.md` for the full command list and `AGENTS.md` for gate costs.

## 1. While writing code — clear the gates by construction

- **Idiomatic Rust**: Prefer standard idioms, compile-time checks, and safety. Avoid unwrap/expect in runtime code unless it's in initial startup/setup where failure should be panic-inducing.
- **Bevy ECS Conventions**:
  - Keep systems decoupled and focused on single responsibilities.
  - Use modular plugins to structure features (e.g. `MovementPlugin`, `CombatPlugin`).
  - Access resources and components cleanly using Bevy's queries.
- **Fast Compiles**: Keep `features = ["dynamic_linking"]` enabled for dev profiles to allow rapid iteration.

Suppressions (like `#[allow(...)]`) must have a clear reason. Never loosen `Cargo.toml` warnings.

## 2. Before you say "done" — run the gate

Do not report success on unverified code. Choose one focused inner loop while
editing, then finish edits and commit normally. The unconditional installed
pre-push hook runs the full `cargo xtask pre-push` gate: workspace fmt,
default/review workspace Clippy and tests, then the native/release/wasm check
matrix without `dev`/`review`. Its successful run satisfies the required gate;
do not duplicate it with an automatic manual invocation before pushing.

Manual `cargo xtask pre-push` remains an explicit early preflight, or the
required fallback when hooks are unavailable. It never suppresses an installed
push hook. Capture the hook's logs instead of rerunning for reporting. See
`AGENTS.md` for observed host costs; no target budget changes are implied.

## 3. Before pushing

Use normal commit/push hooks: cheap hygiene and workspace fmt on commit, the
single authoritative full gate on every push (including config/docs-only
changes). A failed gate blocks the push; fix/recommit and retry. No cached
success receipts or bypasses, including `--no-verify`.

For UI-visible changes, budget for the browser gates: a `web-smoke` scenario is ~10 min, a full baseline capture ~30 min — and always rebase onto `origin/main` **before** capturing baselines so you only capture once.
