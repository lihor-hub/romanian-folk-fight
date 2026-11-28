# Romanian Folk Fight

A browser-based, turn-based arena RPG in the spirit of *Swords and Sandals* —
remastered, and cast entirely from Romanian folklore.

[![CI](https://github.com/lihor-hub/romanian-folk-fight/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/lihor-hub/romanian-folk-fight/actions/workflows/ci.yml)
[![Deploy](https://github.com/lihor-hub/romanian-folk-fight/actions/workflows/deploy.yml/badge.svg?branch=main)](https://github.com/lihor-hub/romanian-folk-fight/actions/workflows/deploy.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![▶ Play now](https://img.shields.io/badge/%E2%96%B6_Play_now-lihor--hub.github.io-2ea44f)](https://lihor-hub.github.io/romanian-folk-fight/)

Build your hero, step into the arena, and fight your way through strigoi,
vârcolaci, and zmei until you face Zmeul Zmeilor himself.

![The arena mid-fight, hero facing a folklore opponent across the sand](docs/media/fight.png)

**Core loop:** fight → earn galbeni → buy gear at the prăvălie → level up →
fight a stronger foe. A boss awaits every 5 fights.

<details>
<summary>More screenshots: character creation and the shop</summary>

| Character creation | Shop (Prăvălia lui Moș Pintea) |
| --- | --- |
| ![Character creation screen with attribute allocation and a live cutout preview](docs/media/creation.png) | ![Shop screen offering folk-themed gear for galbeni](docs/media/shop.png) |

</details>

## Controls

In the browser, give the game focus if keys are going to the browser's own
controls. Click or tap the on-screen buttons, or use the gold focus ring to
select them with the keyboard. **The published release web build uses these
UI controls for combat, not the debug number shortcuts below.**

| Key | Action | Where it applies |
| --- | --- | --- |
| `Tab` / `Shift+Tab` | Focus the next / previous control. | Menu, creation, town, fight, result, shop, victory/game-over screens, and open overlays. |
| `Right` / `Down` | Focus the next control in the same linear order as `Tab`. | The same UI controls; these keys do **not** move the fighter directly. |
| `Left` / `Up` | Focus the previous control, like `Shift+Tab`. | The same UI controls; this is not two-dimensional directional navigation. |
| `Enter` / `Space` | Activate the focused button, like a click. | UI controls, including combat actions and phone action categories. Disabled controls stay focusable but do not perform their action. |
| `Escape` | Close settings and return to the underlying screen/overlay. | While settings are open. If opened from a paused fight, the fight **stays paused**; this keypress does not also resume it. |
| `Escape` | Pause / resume the fight. | During a fight with settings closed. Use a separate press after closing settings to resume. It is not a general Back shortcut on other screens. |
| `M` | Toggle master mute. | All screens, including overlays; saved with the audio settings for later visits on this browser/device, separately from the run save. |

An open settings or pause overlay confines focus to its controls. Moving
focus also scrolls a focused control into view when needed. In creation,
choose a custom hero's name with the on-screen `<` / `>` buttons (focus and
activate them with the keys above): names cycle through a curated list,
not free-text entry. Pressing `M` does not type a name.

With a recognized gamepad, D-pad right/down selects the next control,
left/up selects the previous one, and the south (bottom face) button
activates it. Mouse wheel/trackpad scrolling and touch dragging also scroll
the creation/shop panels on short viewports.

### Debug-build combat shortcuts

**Debug builds only** (`debug_assertions` enabled, such as a normal native
development run), **not the published release web build**. These top-row
keys request actions only during an unpaused, unfinished duel on the
player's turn, while combat animations are idle. Normal play uses the HUD
buttons via pointer/touch or focus navigation instead.

| Key | Action | Context |
| --- | --- | --- |
| `1` | Quick strike — Lovitură iute | Debug fight only |
| `2` | Heavy strike — Lovitură grea | Debug fight only |
| `3` | Block — Apărare | Debug fight only |
| `4` | Rest — Odihnă | Debug fight only |
| `5` | Step forward — Pas înainte | Debug fight only |
| `6` | Step back — Pas înapoi | Debug fight only |
| `7` | Leap forward — Salt înainte | Debug fight only |
| `8` | Normal strike — Lovitură dreaptă | Debug fight only |

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

The native CI commands, verbatim (both default and review configurations):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features review -- -D warnings
cargo test --workspace
cargo test --workspace --features review
```

The wasm CI job additionally runs:

```bash
cargo clippy --target wasm32-unknown-unknown -- -D warnings
trunk build --release
```

Installed hooks keep commits cheap: generic hygiene and workspace format
checking only. Every push, including config/docs-only changes, runs one
unconditional `cargo xtask pre-push`: all five native commands above plus
the native/release/wasm **check** matrix, with no `dev`/`review` in the matrix.
Review lint/tests are local requirements, not CI-only. The separate CI wasm
bundle build and browser/asset workflows remain unchanged.

A successful canonical push hook satisfies the required local full gate;
do not run it manually first just to repeat it at the same inputs. An
explicit early preflight (`cargo xtask pre-push`) is available, but the push
hook still runs unconditionally afterward—there is no cached-success bypass.
`cargo xtask --help` lists the focused verification loops (`test logic`,
`test journey`, `check build-matrix`). The `cargo xtask` alias is generated by
`scripts/bootstrap-worktree.sh`; see `xtask/README.md` for the commands and
`docs/feedback-budgets.md` for their measured timings and target budgets.

## Roadmap

Development is organized into phased milestones — foundation, core loop,
combat, progression & economy, remastered presentation, web release, and
polish. See the
[milestones](https://github.com/lihor-hub/romanian-folk-fight/milestones) and
[issues](https://github.com/lihor-hub/romanian-folk-fight/issues) for the
full plan.

## Contributing

Contributions are welcome — code, art, folklore expertise, and playtesting
all help. Start with [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow, and
please follow the [Code of Conduct](CODE_OF_CONDUCT.md); see
[SECURITY.md](SECURITY.md) to report a vulnerability privately.

We're especially looking for:

- Rust/Bevy code — gameplay systems, UI, tooling
- Art & audio in the folk-textile visual style (see `docs/art-direction.md`)
- Romanian folklore and language review (names, flavor text, announcer lines)
- Playtesting and game design feedback
- Documentation

Good places to start:

- [`good first issue`](https://github.com/lihor-hub/romanian-folk-fight/labels/good%20first%20issue) label
- [Welcome discussion](https://github.com/lihor-hub/romanian-folk-fight/discussions/350)
- [Design ideas thread](https://github.com/lihor-hub/romanian-folk-fight/discussions/351)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — a newcomer's tour of the codebase

## License

Dual-licensed under either:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Assets are licensed separately per-file; see [CREDITS.md](assets/CREDITS.md) for details.

## Automatic Scripts
For more details on running our asset collection pipelines, see our [Scripts Directory Index](scripts/README.md).
