# Architecture tour

A newcomer's map of the codebase: what runs first, how the game is split into
plugins, where each piece of gameplay lives, and where the tests are. It
describes what's actually in the tree today — see each module's own doc
comment (the `//!` block at the top of its `mod.rs`) for the authoritative,
more detailed version.

## App entry and plugin registration

The binary is intentionally thin. [`src/main.rs`](../src/main.rs) configures
the Bevy `App`'s window (title, canvas element, letterboxing) and asset
loading, then adds a single plugin:

```rust
App::new()
    .add_plugins(DefaultPlugins.set(/* window + asset config */))
    .add_plugins(GamePlugin)
    .run();
```

`GamePlugin` lives in [`src/lib.rs`](../src/lib.rs) and is the actual game.
Putting it in the library crate (rather than the binary) is deliberate: it
makes every plugin unit-testable with a headless `App` and `MinimalPlugins`,
without spinning up a real window or renderer. `GamePlugin::build` is the
single place that wires up every feature plugin, in this order:

```rust
app.add_plugins(core::CorePlugin);
app.add_plugins(flow::FlowPlugin);
app.add_plugins(character::CharacterPlugin);
app.add_plugins(cutout::CutoutRigPlugin);
app.add_plugins(menu::MenuPlugin);
app.add_plugins(creation::CreationPlugin);
app.add_plugins(items::ItemsPlugin);
app.add_plugins(arena::ArenaPlugin);
app.add_plugins(combat::CombatPlugin);
app.add_plugins(announcer::AnnouncerPlugin);
app.add_plugins(audio::GameAudioPlugin);
app.add_plugins(progression::ProgressionPlugin);
app.add_plugins(roster::RosterPlugin);
app.add_plugins(save::SavePlugin);
app.add_plugins(settings::SettingsPlugin);
app.add_plugins(shop::ShopPlugin);
app.add_plugins(town::TownPlugin);
#[cfg(feature = "review")]
app.add_plugins(review::ReviewPlugin);
```

A unit test in `src/lib.rs` (`game_plugin_builds_without_duplicate_plugins`)
just builds this app against `MinimalPlugins` — a cheap regression check that
every plugin's `build()` is compatible with every other's (no duplicate
resource/message registration, no missing dependency).

## One plugin per feature

Every gameplay slice is its own Bevy plugin in its own top-level module under
`src/`. The convention: a module owns its states' entities, systems, and
resources, and only reaches into another module's *public* types (re-exported
from that module's `mod.rs`). There's no central "god" system or resource —
cross-cutting concerns (navigation, styling, save format) are their own
modules that everyone else depends on, described below.

## Module map

| Module | Owns |
| --- | --- |
| [`core`](../src/core/mod.rs) | `GameState` (the top-level screen enum), the letterboxed camera/projection math, and `despawn_screen` — the shared per-state cleanup helper every screen uses. |
| [`flow`](../src/flow/mod.rs) | The single validated state-transition table (`FlowIntent` → `GameState`). The *only* runtime code allowed to write `NextState<GameState>`; every screen emits an intent instead (enforced by an AST-based regression test). |
| [`character`](../src/character/mod.rs) | The character model: attributes, resource pools, the part-based catalog system (`catalog`), procedural generation (`generation`), and fighter markers shared by combat/creation/shop/progression. |
| [`cutout.rs`](../src/cutout.rs) | Runtime rendering of the pixel-art cutout rig — body parts and gear attached to creator previews and arena fighters. |
| [`menu`](../src/menu/mod.rs) | The main menu/title screen, and the reusable button-interaction pattern (`MenuAction`) later screens copy. |
| [`creation`](../src/creation/mod.rs) | Character creation: preset or custom hero, attribute/appearance editing, live cutout preview. Allocation rules live in `creation::draft` as pure, Bevy-free logic. |
| [`items`](../src/items/mod.rs) | The static equipment catalog (`catalog`) and item visuals (`visuals`); combat only ever sees the aggregated damage/armor bonus, never individual items. |
| [`town`](../src/town/mod.rs) | The between-fights hub screen: one dominant "enter the arena" action plus the shop and a read-only character view. |
| [`arena`](../src/arena/mod.rs) | The fight scene: scenery, the player's and current ladder opponent's animated sprite fighters (`animation`), and visual effects (`fx`). |
| [`combat`](../src/combat/mod.rs) | Turn-based combat: a pure seeded-RNG resolution core (`engine`), a pure enemy decision policy (`ai`), the ECS glue (`systems`), the fight HUD (`hud`), the action palette (`action_palette`), and the in-fight pause overlay (`pause`). |
| [`announcer`](../src/announcer/mod.rs) | The louder, folk-humored banner layer over the same combat event stream the HUD log reads; picks Romanian one-liners from `lines`. |
| [`audio`](../src/audio/mod.rs) | State-driven music (menu/arena/boss/silent-result themes) and SFX reacting to combat/UI events. |
| [`progression`](../src/progression/mod.rs) | Run currency (galbeni), fight outcomes, the level curve (`level`), and the result/victory screens (`result_ui`, `victory_ui`) that route back through `flow`. |
| [`roster`](../src/roster/mod.rs) | The folklore opponent ladder: ten creatures of escalating difficulty with a boss every five fights, looping with a per-lap attribute bump. Plain static data. |
| [`shop`](../src/shop/mod.rs) | The shop screen ("Prăvălia lui Moș Pintea"): spending galbeni on catalog gear between fights, with pure purchase rules (`try_buy`) kept separate from the UI systems. |
| [`save`](../src/save/mod.rs) | Run persistence: a versioned snapshot schema (`snapshot`), the storage backends (`storage`), and the autosave/delete wiring. |
| [`settings`](../src/settings/mod.rs) | The settings overlay (volume, mute, reduced-motion, high-contrast) — an overlay, not a `GameState`, so opening it never disrupts a paused fight. |
| [`theme`](../src/theme/mod.rs) | The single source of truth for UI styling: the folk-textile palette, spacing, fonts, button styles, and the 9-slice panel texture (see `tokens` for palette/contrast tokens). |
| [`ui_widgets`](../src/ui_widgets/mod.rs) | Shared UI building blocks used by more than one screen: button bundles, the attribute allocation row, and wheel/touch scroll behavior. |
| [`review`](../src/review/mod.rs) (feature-gated) | The browser-harness seam used only by the automated web-smoke gold-journey build; see [below](#the-review-feature). |

## Game flow and screen transitions

`GameState` (in `core::mod`) is the top-level enum every screen scopes its
systems and entities to: `Loading` → `MainMenu` / `CharacterCreation` →
`Town` (the hub) → `Fight` / `Shop` → `FightResult` / `GameOver` / `Victory`,
looping back through `Town`.

The `flow` module owns *every* transition between these states as one
explicit table (`transition_for` in [`src/flow/mod.rs`](../src/flow/mod.rs)):
a `(current state, FlowIntent)` pair maps to a next state, or is rejected. A
screen never calls `NextState::set` directly — it performs its own domain
side effect first (crediting a reward, resetting the run, restoring a save),
then emits a `FlowIntent`, and `flow::apply_flow_intents` is the sole system
that actually changes state. This is enforced, not just documented: a test in
that module parses every `.rs` file in `src/` with `syn` and fails the build
if any production code outside `src/flow/` writes `NextState<GameState>`
(with one pre-existing, named exception for the loading-screen bootstrap
gate in `core`).

To add a new navigation route, extend `FlowIntent` and `transition_for`
together — the module's doc comment spells out the exact procedure and the
tests to add.

## Where game data lives

- **Items** ([`src/items/catalog.rs`](../src/items/catalog.rs)): the full
  equipment catalog is plain static Rust data (`CATALOG`). The shop sells
  from it; combat reads only the aggregated `total_damage_bonus()` /
  `total_armor()` off `FighterState`.
- **Roster** ([`src/roster/mod.rs`](../src/roster/mod.rs)): the ten-opponent
  folklore ladder (with a boss every five) is likewise static data, with
  attribute budgets enforced by unit tests rather than hand-tuned per fight.
- **Characters** ([`src/character/catalog.rs`](../src/character/catalog.rs)):
  the part-based human catalog (`assets/fighters/catalog/human-foundation.json`)
  describes body parts, wardrobe, and attachment points; `character::generation`
  resolves a concrete `CharacterDefinition` from it deterministically.

## Saves

`save` ([`src/save/mod.rs`](../src/save/mod.rs)) persists a versioned JSON
snapshot of every run-scoped resource. The module is split three ways:

- `snapshot` owns the schema, version envelope, and migration/capture/restore
  contract — it never touches a filesystem or `localStorage`.
- `storage` owns *where* the JSON physically lives: on the web build it's
  `window.localStorage` (key `rff_save_v1`); natively it's a file under
  `dirs::data_dir()/romanian-folk-fight/save.json`, written via a
  same-directory temp file plus an atomic rename (never a torn in-place
  write).
- the top-level `save` module wires autosave requests (each carrying a
  `ResumeDestination` — arena, shop, or town) and the game-over delete.

A run is one life: losing deletes the save. The main menu's **Continuă**
button restores the snapshot and resumes at whichever screen the save's
`resume_destination` points to.

## Theme and shared UI widgets

`theme` ([`src/theme/mod.rs`](../src/theme/mod.rs)) is the single source of
truth for visual styling — the Romanian folk-textile color palette, spacing
scale, text presets, button style bundles, and the embroidery-motif 9-slice
panel texture. No screen defines its own color literals; the convention is
strict enough that `grep -rn "Color::srgb" src/ --include=*.rs` outside this
module returns nothing (see `docs/art-direction.md` for the palette source).
`theme::tokens` layers palette/contrast tokens (including the high-contrast
accessibility variant) on top.

`ui_widgets` ([`src/ui_widgets/mod.rs`](../src/ui_widgets/mod.rs)) holds the
handful of UI building blocks shared by more than one screen: the button
bundle pattern the main menu established, the attribute +/- allocation row
(used by both character creation and the level-up panel), keyboard focus
navigation (`focus`), and wheel/touch-drag scrolling shared by the shop and
creation screens.

## The `review` feature

`src/review/mod.rs` is compiled in only behind the `review` Cargo feature —
`#[cfg(feature = "review")]` end to end, so it is structurally absent from an
ordinary `cargo build`, `cargo build --release`, or `trunk build --release`
artifact, not merely hidden behind a runtime flag. It implements a
`window.localStorage`-based bridge that lets the automated browser-smoke
harness (`cargo xtask web-smoke --scenario gold-journey`) seed a
deterministic combat RNG, pick a character-creation preset, and drive screen
navigation through the same `FlowIntent`/button-press machinery a real player
uses — instead of pixel-coordinate clicking or relying on random play. This
is what makes the gold-journey visual baselines (`tests/visual/baselines/`)
reproducible run to run. Because it's feature-gated, both `cargo xtask
pre-push` and CI run `clippy`/`test` twice — once with default features, once
with `--features review` — so this seam and its tests stay covered even
though it never ships.

## Native vs. wasm builds

The game targets both a native binary (fast local iteration) and a
WebAssembly + WebGL2 build served via [Trunk](https://trunkrs.dev/) (what
actually ships to <https://lihor-hub.github.io/romanian-folk-fight/>). A few
things differ by platform:

- **Save storage**: native writes to a data-dir file; wasm writes to
  `window.localStorage` (see [Saves](#saves) above).
- **Asset meta checks**: `main.rs` disables Bevy's `.meta` file check
  (`AssetMetaCheck::Never`) because no `.meta` files ship with the assets,
  and on a wasm dev server a missing `.meta` request otherwise gets answered
  with the SPA fallback page, breaking the load.
- **Audio autoplay**: on wasm, the browser blocks audio until the first user
  interaction (click/key/touch); native starts music immediately. See
  `audio`'s module docs.
- **The `dev` feature** (`bevy/dynamic_linking`) is for fast native
  incremental builds only (`cargo run --features dev`) and must never leak
  into a release or wasm build — `cargo xtask check build-matrix` checks a
  plain native build, `--release`, and `--target wasm32-unknown-unknown`,
  none of them with `dev` enabled.
- **The `review` feature** (previous section) is only ever built into the
  dedicated gold-journey wasm bundle, never `dist/`.

## Tests

- **Unit tests** live inline in each module (`#[cfg(test)] mod tests` at the
  bottom of the relevant file) — pure logic (damage formulas, AI choice, the
  creation draft, item/roster data, the level curve) alongside headless
  Bevy-`App` tests that exercise a plugin's systems with `MinimalPlugins`.
- **`cargo xtask test logic`** runs just the pure, non-Bevy-`App` subset
  (`character::`, `combat::ai::`, `combat::engine::`, `creation::draft::`,
  `items::`, `progression::level::`, `roster::`) — the fastest loop for
  iterating on game-rule changes, skipping the ~250 tests that each boot a
  headless `App`.
- **`cargo xtask test journey`** targets the closest existing full headless
  `GameState` journey test, driving fight → result → fight → result → game
  over and back to the menu through real button presses.
- **Visual regression** lives under `tests/visual/baselines/`: PNG
  screenshots captured by `cargo xtask web-smoke --scenario gold-journey`
  (and the other web-smoke scenarios) at fixed viewports, compared against
  committed baselines. This is what `docs/media/`'s screenshots in the
  project [README](../README.md) are copied from.
- **`cargo xtask pre-push`** is the full local gate: `fmt`, `clippy` and
  `test` each run twice (default features, then again with `--features
  review`), plus the native/release/wasm build matrix. See
  [`xtask/README.md`](../xtask/README.md) for every command and
  [`docs/feedback-budgets.md`](feedback-budgets.md) for their timings.
