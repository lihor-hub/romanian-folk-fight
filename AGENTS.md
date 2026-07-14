# Agent Notes

## Infrastructure Knowledge

- The application is a Game built with Rust and the Bevy game engine.
- Runtime code should use Bevy's ECS (Entity Component System) conventions.
- Organize game logic into modular Bevy plugins.
- Use explicit dependencies and features in `Cargo.toml`.
- For development builds, ensure we enable fast compilation optimizations.
- Use `cargo run --features dev` for fast native iteration (Bevy dynamic linking); plain `cargo build`/`--release` and wasm builds must stay free of the `dev` feature.
- Web build: `rustup target add wasm32-unknown-unknown`, `cargo install trunk` (or `brew install trunk`), then `trunk serve` for a local browser build and `trunk build --release` for a distributable `dist/`.

## Verification Gates and Costs

Budget for these before starting; they dominate wall-clock time.

- `cargo xtask pre-push` (fmt, clippy, test, build-matrix) — ~4 min; required before every push.
- `cargo xtask web-smoke --scenario <name>` — builds and serves the wasm game in a real browser; ~10 min per scenario.
- `cargo xtask web-smoke --all` / the gold-journey desktop+phone DPR matrix — ~30 min per full baseline capture.
- Rebase onto `origin/main` **before** capturing or re-accepting visual baselines, never after — a rebase that changes rendered UI invalidates baselines and forces a full re-capture.
- `cargo xtask test logic` is the fast loop for pure game-rule changes; use it while iterating, save the full gate for the end.

## Parallel Agent Work

- Run at most 2 implementation agents concurrently; the session limit kills all in-flight agents when exhausted.
- Issues touching the same subsystem (HUD, theme, action palette) must be worked sequentially — parallel PRs there collide, and the rebase + baseline re-capture costs more than the parallelism saves.
- See the `orchestrate` skill for the full multi-agent protocol.

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
