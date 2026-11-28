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

- Every push requires the full `cargo xtask pre-push` gate: workspace fmt, default/review workspace clippy+tests, then the native/release/wasm check matrix without `dev`/`review`. The installed unconditional pre-push hook runs it once, including config/docs-only pushes; its success satisfies this requirement. Do not duplicate it with an automatic manual run first. Manual preflight is optional for earlier feedback (the hook still reruns); if hooks are unavailable, run the command manually. Never bypass installed hooks or reuse a cached-success receipt. Observed Linux full-gate steps took 12–14 min, with ~6 min of launcher compilation in several runs; host/cache costs vary, and target budgets are unchanged.
- `cargo xtask web-smoke --scenario gold-journey` — the fast browser loop: one 1440x900 DPR-1 journey across all five current screens; ~10 min from a cold wasm build.
- `cargo xtask web-smoke --scenario <name>` — runs one specialized phone, zoom, touch, accessibility, persistence, or rendering scenario; ~10 min per scenario.
- `cargo xtask web-smoke --all` — runs every registered scenario; reserve it for explicit broad verification.
- Rebase onto `origin/main` **before** capturing or re-accepting visual baselines, never after — a rebase that changes rendered UI invalidates baselines and forces a full re-capture.
- Choose one relevant inner loop: a focused test or `cargo xtask test logic` for broad pure-rule changes. Do not append redundant suites to equivalent passing evidence at unchanged inputs. Save the full gate for the canonical push hook; retain logs and preserve warm targets/flags.

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
