# Agent Notes

## Infrastructure Knowledge

- The application is a Game built with Rust and the Bevy game engine.
- Runtime code should use Bevy's ECS (Entity Component System) conventions.
- Organize game logic into modular Bevy plugins.
- Use explicit dependencies and features in `Cargo.toml`.
- For development builds, ensure we enable fast compilation optimizations.
- Use `cargo run --features dev` for fast native iteration (Bevy dynamic linking); plain `cargo build`/`--release` and wasm builds must stay free of the `dev` feature.

## Git Workflow

- Before starting work, fetch and rebase on `origin/main`.
- Keep working branches fast-forwardable with `origin/main`; resolve divergence by rebasing rather than merging.
- Do not use `git push --no-verify` when pushing changes.

## Agent Skills

- Keep Claude and Codex skill access synchronized at all times.
- `.agents/skills` must point at `.claude/skills` so Codex agents see the same project skills without maintaining duplicate copies.
- `CLAUDE.md` must be a symlink to `AGENTS.md` so Claude Code loads these instructions too; edit `AGENTS.md` only.

## Worktrees

- Fresh worktrees lack build target cache and bootstrap scripts; keep cargo tooling on `PATH`.
