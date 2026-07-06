# Romanian Folk Fight

A browser-based, turn-based arena RPG in the spirit of *Swords and Sandals* —
remastered, and cast entirely from Romanian folklore. Build your hero, step
into the arena, and fight your way through strigoi, vârcolaci, and zmei until
you face Zmeul Zmeilor himself.

**Core loop:** fight → earn galbeni → buy gear at the prăvălie → level up →
fight a stronger foe. A boss awaits every 5 fights.

## Tech stack

- [Rust](https://www.rust-lang.org/) + [Bevy 0.19](https://bevy.org/) — ECS
  architecture, one plugin per feature.
- WebAssembly + WebGL2 via [Trunk](https://trunkrs.dev/) for the browser
  build; native builds for day-to-day development.

## Getting started

Prerequisites: [rustup](https://rustup.rs/) and (optional, for git hooks)
[pre-commit](https://pre-commit.com/).

```bash
git clone https://github.com/lihor-hub/romanian-folk-fight.git
cd romanian-folk-fight
scripts/bootstrap-worktree.sh   # verifies cargo, installs git hooks
```

### Run natively (fastest iteration)

```bash
cargo run --features dev
```

The `dev` feature enables Bevy dynamic linking for fast incremental builds.
Never enable it for release or wasm builds.

### Run in the browser

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk        # or: brew install trunk
trunk serve                # serves on http://localhost:8080
```

`trunk build --release` produces a distributable bundle in `dist/`.

## Quality gates

CI enforces all of these on every PR; the pre-commit/pre-push hooks mirror
them locally:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Roadmap

Development is organized into phased milestones — foundation, core loop,
combat, progression & economy, remastered presentation, web release, and
polish. See the
[milestones](https://github.com/lihor-hub/romanian-folk-fight/milestones) and
[issues](https://github.com/lihor-hub/romanian-folk-fight/issues) for the
full plan.

## License

Not yet chosen — tracked in
[#33](https://github.com/lihor-hub/romanian-folk-fight/issues/33). Asset
licenses will be recorded per-file in `assets/CREDITS.md` as art and audio
land.
