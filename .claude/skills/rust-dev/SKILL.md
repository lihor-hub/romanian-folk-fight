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
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt --all -- --check`
- Test: `cargo test`
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

Do not report success on unverified code. Run, in order, and fix until clean:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Or run the whole gate in one shot (~4 min, stops at first failure):

```bash
cargo xtask pre-push   # fmt check, clippy, cargo test, build-matrix
```

`build-matrix` also proves wasm builds stay free of the `dev` feature — run it whenever `Cargo.toml` features or targets changed.

## 3. Before pushing

Make sure all checks pass. Unit tests are run pre-push (via the git hook). Never bypass hooks with `--no-verify`.

For UI-visible changes, budget for the browser gates: a `web-smoke` scenario is ~10 min, a full baseline capture ~30 min — and always rebase onto `origin/main` **before** capturing baselines so you only capture once.
