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

In a fresh worktree, run `scripts/bootstrap-worktree.sh` to ensure paths and hooks are configured.

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

## 3. Before pushing

Make sure all checks pass. Unit tests are run pre-push (via the git hook). Never bypass hooks with `--no-verify`.
