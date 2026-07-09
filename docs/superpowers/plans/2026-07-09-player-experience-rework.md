# Romanian Folk Fight Player Experience Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. A linked GitHub issue—not a roadmap subsection—is the atomic execution task. Implement exactly one issue per worktree and PR, merge it, then rebase the next dependent issue. Steps use checkbox (`- [ ]`) syntax for program tracking.

**Goal:** Turn the shipped arena ladder into a polished, readable campaign, then extend that stable core into the complete town-centered RPG without a rewrite.

**Architecture:** Preserve the existing Bevy feature plugins and pure gameplay modules. Add four explicit seams: a single flow-intent owner for navigation, a versioned run snapshot for persistence, metadata-driven combat actions/effects, and automated asset/browser review contracts. Deliver the work as dependency-ordered vertical slices tracked by [GitHub issue #151](https://github.com/lihor-hub/romanian-folk-fight/issues/151).

**Tech Stack:** Rust 2024, Bevy 0.19, Trunk, WebAssembly/WebGL2, GitHub Actions, browser screenshot tests, project-owned PNG/OGG assets.

## Global Constraints

- Use Bevy ECS conventions and keep runtime behavior in focused feature plugins.
- Use `cargo run --features dev` only for fast native iteration; plain native, release, and WASM builds must stay free of the `dev` feature.
- Preserve the existing pure rule boundaries in combat, AI, creation, leveling, roster, catalog, and save serialization.
- Keep Romanian copy and diacritics correct in the first painted frame and every later screen.
- Treat 1280×800 desktop and 390×844 phone layouts as equal release targets; validate device pixel ratios 1, 2, and 3.
- Do not route legacy full-frame sprites back into the production cutout runtime.
- Treat the existing Python asset generators as bootstrap-placeholder tooling only; they may not produce accepted production assets.
- Every accepted asset needs explicit provenance, license, crop, scale, pivot, and attachment data where applicable.
- UI and asset changes require rendered before/after evidence; unit tests alone cannot prove presentation quality.
- One issue, one worktree, one independently reviewable PR. Rebase on `origin/main`; never merge feature branches together.
- Do not begin a blocked issue because an agent slot is free. Pull the next unblocked issue from the current wave.

---

## 1. Decision and baseline

This program deliberately includes all three ambitions discussed during discovery:

1. Repair and polish the current ten-fight arena campaign.
2. Deepen the combat and progression systems.
3. Grow the result/shop loop into a town, tutorial, opponent-preview, rematch, and tournament campaign.

The sequence is non-negotiable. The current deployed build has visible first-load, HiDPI, panel, fighter-facing, joint, identity, and HUD composition defects. Adding more content before correcting those foundations would multiply rework.

The repository does not need a rewrite. It already has valuable pure logic modules and extensive unit coverage. Its missing layer is composed-player-experience verification: hundreds of module tests currently coexist with a broken first frame and visibly incorrect fighter rigs.

## 2. Target player journeys

### Gold compact campaign

`cold load → new hero → first fight → result → shop or level choice → next opponent → defeat/reload honesty`

This journey is the release gate before town expansion. A first-time player must understand what to do without reading external instructions.

### Full town campaign

`cold load → new hero → prisoner tutorial → town → choose service or arena → inspect opponent → duel/rematch/tournament → recover/upgrade → resume honestly after reload`

The full campaign reuses the compact campaign's flow, persistence, action, asset, and browser-test contracts. It does not create parallel versions.

## 3. File and ownership map

The following boundaries are the intended outcome. A child issue may adjust a filename during its own approved implementation plan, but it may not create a competing owner for the same responsibility.

| Responsibility | Intended owner | Consumers |
| --- | --- | --- |
| Navigation intents and transition table | `src/flow/mod.rs` | Menu, creation, progression, shop, town, arena select, tutorial |
| Top-level states, cameras, viewport | `src/core/mod.rs` | Flow and every rendered screen |
| Versioned run capture/restore/migration | `src/save/snapshot.rs` | Native/web save backends, flow, tutorial, tournament |
| Save storage only | `src/save/mod.rs` | Snapshot API |
| Action descriptors and categories | `src/combat/actions.rs` | Combat rules, HUD, tutorial, opponent preview |
| Timed effects and expiry rules | `src/combat/effects.rs` | Combat engine, spells, consumables, armour, HUD |
| Responsive action palette | `src/combat/action_palette.rs` | Fight HUD |
| Shared responsive screen shell | `src/ui_widgets/screen_shell.rs` | Result, game over, victory, town, shop, preview |
| Runtime asset contract | `assets/manifest.toml` | Rust asset paths, credits, gallery, CI |
| Command dispatcher and shared `xtask` wiring | `xtask/src/main.rs`, root Cargo registration | Owned by #152; consumed by asset/browser tooling |
| Asset validation and gallery subcommands | `xtask/src/assets/` | Asset agents and CI |
| Browser-smoke orchestration subcommands | `xtask/src/web_smoke/` | UI/web agents and CI |
| Browser journeys and screenshot baselines | `tests/visual/` | UI, assets, viewport, web runtime |
| Runtime fighter parts | `assets/fighters/<identity>/runtime/` | Cutout rig |
| Location source/runtime layers | `assets/locations/<location>/` | Town, tutorial, arena scenes |

Large existing screen modules should move behavior into these owners while they are touched. Do not perform unrelated file splitting as a standalone cleanup campaign.

## 4. Feedback loop contract

### Worktree bootstrap

Every implementation agent starts with:

```bash
scripts/bootstrap-worktree.sh
git fetch origin
git rebase origin/main
git status --short --branch
```

Before writing a test or asset, the agent must also:

1. Read the assigned issue body and every issue comment.
2. Read all blocking issues and confirm their PRs are merged into the current base.
3. Read [#151](https://github.com/lihor-hub/romanian-folk-fight/issues/151), this plan, `AGENTS.md`, and `docs/art-direction.md`.
4. Restate the issue's player-visible before/after, focused verification command, and any program-level correction that overrides stale issue text.
5. Stop if the assigned issue conflicts with a merged owner contract; do not invent a parallel API.

[Issue #152](https://github.com/lihor-hub/romanian-folk-fight/issues/152) must make this reliable when Cargo is initially absent from `PATH`, configure shared compiler caching, and retain separate target directories so parallel worktrees do not fight over one Cargo lock.

### Inner loops

Use the smallest loop that can disprove the current edit:

```bash
# Pure or headless Rust rule
cargo test <module-or-test-filter> --lib

# Asset path, provenance, crop, pivot, and palette contract
cargo xtask assets check

# Changed asset and dependent-composition gallery
cargo xtask assets review --changed

# One deterministic browser journey
cargo xtask web-smoke --scenario <scenario-name>
```

[Issue #152](https://github.com/lihor-hub/romanian-folk-fight/issues/152) owns the root `xtask` crate, command dispatcher, Cargo registration, and shared process/error conventions. After #152 merges, [#141](https://github.com/lihor-hub/romanian-folk-fight/issues/141) adds the asset subcommands and [#144](https://github.com/lihor-hub/romanian-folk-fight/issues/144) adds browser-smoke orchestration in disjoint modules. Those command names are the shared interface; later agents add scenarios and checks without creating new runners.

Target warm budgets:

| Loop | Budget | Failure output |
| --- | ---: | --- |
| Focused pure/headless test | 30 seconds | Test name and assertion diff |
| Asset contract | 5 seconds | Asset id, violated field, source/runtime paths |
| Changed-asset gallery | 30 seconds | Local gallery path and affected compositions |
| One browser scenario | 90 seconds | Scenario, viewport/DPR, console errors, screenshot diff |
| Full pre-push gate | 10 minutes | First failing command plus retained artifacts |

Budgets are observability thresholds, not excuses to skip verification. [Issue #152](https://github.com/lihor-hub/romanian-folk-fight/issues/152) records representative cold and warm measurements.

### Required pre-review gate

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo clippy --target wasm32-unknown-unknown -- -D warnings
trunk build --release
```

Add the affected asset and browser commands for changes that touch UI, assets, cameras, states, loading, or web behavior.

## 5. Agent proof bundle

Every PR description must contain:

1. The assigned issue and blockers confirmed merged.
2. One sentence describing the player-visible before and after.
3. The failing test observed before implementation.
4. Focused and full verification commands with exit results.
5. UI proof when UI/rendered output changed: before/after at 1280×800 and 390×844, with DPR 2 included; otherwise state `Not applicable — no rendered output changed`.
6. Asset proof when assets or asset composition changed: changed-assets gallery and provenance record; otherwise state `Not applicable — no assets changed`.
7. Known limitations that are explicitly assigned to another issue.

Two reviews answer separate questions:

- **Spec review:** every acceptance criterion is met, no blocker contract is bypassed, and scope did not leak.
- **Player-proof review:** the rendered journey is visibly correct, readable, and better than the baseline.

An issue remains open until both reviews pass.

## 6. Wave 0 — Feedback spine

Run the first row in parallel because the files are disjoint:

- [ ] [#142 Central flow intents](https://github.com/lihor-hub/romanian-folk-fight/issues/142)
- [ ] [#152 Fast, self-verifying worktrees](https://github.com/lihor-hub/romanian-folk-fight/issues/152)

After #152 merges, rebase and run the second row in parallel. #141 and #144 own separate `xtask` modules and separate workflow files; neither may edit the other's module.

- [ ] [#141 Asset manifest and review gallery](https://github.com/lihor-hub/romanian-folk-fight/issues/141)
- [ ] [#144 Browser visual smoke tests](https://github.com/lihor-hub/romanian-folk-fight/issues/144)

### Wave 0 integration check

- [ ] Run the asset validator against the current repository and record every known failure as a linked issue rather than weakening the validator.
- [ ] Drive menu → creation → fight → result/shop → next fight and defeat → reset in the headless flow harness.
- [ ] Capture cold menu, creation preset, first fight, result, and shop at the required viewport/DPR matrix.
- [ ] Confirm a broken asset path produces one focused asset failure and one focused browser failure.
- [ ] Confirm two worktrees can run focused Rust tests concurrently without sharing a target lock.

Wave 1 does not begin until the commands exist, even if their first runs expose known failures.

## 7. Wave 1 — First impression and layout correctness

Each issue below is a separate worktree and PR. Merge and rebase at the end of every numbered issue sequence.

### Issue sequence 1.1: Cold first frame

- [ ] Implement [#114](https://github.com/lihor-hub/romanian-folk-fight/issues/114).
- [ ] Prove the loading gate waits for the font, panel, and any first-screen motif assets.
- [ ] Capture the very first menu frame with cache disabled; waiting and navigating away/back is not acceptable evidence.

### Issue sequence 1.2: Logical pixels

- [ ] Implement [#115](https://github.com/lihor-hub/romanian-folk-fight/issues/115).
- [ ] Validate desktop/phone at DPR 1, 2, and 3 and confirm the world camera remains correctly letterboxed.

### Issue sequence 1.3: Panel integrity

- [ ] Implement and merge [#119](https://github.com/lihor-hub/romanian-folk-fight/issues/119).
- [ ] Rebase, then implement and merge [#120](https://github.com/lihor-hub/romanian-folk-fight/issues/120) against #119's final panel contract.

### Issue sequence 1.4: HUD bounds and identity

- [ ] Implement [#125](https://github.com/lihor-hub/romanian-folk-fight/issues/125) against #115, #119, and #120.
- [ ] Implement [#127](https://github.com/lihor-hub/romanian-folk-fight/issues/127).

### Issue sequence 1.5: Palette, accessibility, and menu motifs

- [ ] Implement [#126](https://github.com/lihor-hub/romanian-folk-fight/issues/126).
- [ ] Implement [#145](https://github.com/lihor-hub/romanian-folk-fight/issues/145).
- [ ] Implement [#121](https://github.com/lihor-hub/romanian-folk-fight/issues/121) through the asset manifest and loading gate.

### Wave 1 exit evidence

- [ ] No required screen unexpectedly scrolls at 1280×800 or 390×844.
- [ ] Alegreya, Romanian diacritics, and embroidered panels are present on the cold first frame.
- [ ] Panel motifs tile; content clears the border; fighter names appear once; HUD stays inside the arena.
- [ ] Keyboard focus, 200% browser zoom, WCAG text/non-text contrast targets, reduced motion, and 44×44 CSS-pixel minimum touch targets pass.

## 8. Wave 2 — Fighter and asset truthfulness

Each issue below is a separate worktree and PR. Merge #116 before starting #117, merge #117 before #123, and merge #148 before #118.

### Issue sequence 2.1: Correct the rig contract

- [ ] Implement [#116](https://github.com/lihor-hub/romanian-folk-fight/issues/116) so transforms, body sprites, and gear sprites use the same facing.
- [ ] Implement [#117](https://github.com/lihor-hub/romanian-folk-fight/issues/117) with explicit joint hierarchy and asset pivots from the manifest.
- [ ] Add pose-level gallery scenarios for idle, quick/normal/heavy attack, guard, dodge, hurt, and KO.

### Issue sequence 2.2: Make creation truthful

- [ ] Implement [#123](https://github.com/lihor-hub/romanian-folk-fight/issues/123).
- [ ] Prove skin, hair, body, accent, and preset choices change the rendered preview and the fight rig consistently.
- [ ] Confirm the preview is grounded, unclipped, and responsive.

### Issue sequence 2.3: Complete roster identities

- [ ] Implement [#148](https://github.com/lihor-hub/romanian-folk-fight/issues/148) to produce reviewed runtime assets.
- [ ] Implement [#118](https://github.com/lihor-hub/romanian-folk-fight/issues/118) to select those assets and apply roster identity data.
- [ ] Reject any implementation that routes legacy full-frame sheets into the production cutout path.
- [ ] Review silhouettes in grayscale, at thumbnail size, both facings, and at real game scale.

### Wave 2 exit evidence

- [ ] Fighters face each other and all gear is on the correct body side.
- [ ] No elbow, wrist, knee, ankle, weapon, or shield detaches in any accepted pose.
- [ ] Every ladder opponent is intentional and adjacent opponents differ at silhouette level.
- [ ] Every runtime asset has explicit provenance and passes the gallery review.

## 9. Wave 3 — Gold compact campaign

Each issue below is a separate worktree and PR. Merge and rebase in the listed order.

### Issue sequence 3.1: Scalable action presentation

- [ ] Implement [#143](https://github.com/lihor-hub/romanian-folk-fight/issues/143).
- [ ] Implement [#122](https://github.com/lihor-hub/romanian-folk-fight/issues/122) through action descriptors and publish a pictogram style contract covering canvas size, outline weight, silhouette rules, shared palette, disabled treatment, and small-size readability.
- [ ] Implement [#124](https://github.com/lihor-hub/romanian-folk-fight/issues/124) through the same descriptor/view-data path.
- [ ] Prove a test-only action appears without modifying HUD layout code.

### Issue sequence 3.2: Honest run persistence

- [ ] Implement [#146](https://github.com/lihor-hub/romanian-folk-fight/issues/146) after #142.
- [ ] Migrate version-1 saves once, preserving fields that existed and assigning explicit safe defaults to new fields.
- [ ] Prove abandon/continue cannot turn a losing fight into a free full-health retry.
- [ ] Treat the #146 schema as the first extensible snapshot version, not the final campaign schema; every later persistent field requires a new version, a migration from the previous version, and a safe default backed by a migration test.

### Issue sequence 3.3: Gold journey proof

- [ ] Run cold load → new hero → first fight → result → shop/level choice → next opponent.
- [ ] Run a defeat and honest reload path from a fresh browser profile.
- [ ] Record comprehension problems as new issues; do not hide them in the PR description.

Wave 4 does not begin until the gold journey passes headless and browser gates.

## 10. Wave 4 — Combat depth

Every Wave 4 issue is sequential by default because combat actions still share exhaustive rule, AI, event, audio, announcer, and catalog owners. Parallel work requires a separately reviewed registration seam that makes the file ownership genuinely disjoint.

### Issue sequence 4.1: Final core models

- [ ] Implement [#128](https://github.com/lihor-hub/romanian-folk-fight/issues/128), explicitly allowing `magie == 0` non-casters.
- [ ] Implement [#134](https://github.com/lihor-hub/romanian-folk-fight/issues/134) as the only distance/reach model.
- [ ] Implement [#150](https://github.com/lihor-hub/romanian-folk-fight/issues/150) as the only temporary-effect lifecycle.

### Issue sequence 4.2: Actions on stable contracts

After Issue sequence 4.1, merge and rebase each of the following in order:

- [ ] [#130 Strike tiers](https://github.com/lihor-hub/romanian-folk-fight/issues/130)
- [ ] [#131 Taunt and shove](https://github.com/lihor-hub/romanian-folk-fight/issues/131)
- [ ] [#132 Folk spells](https://github.com/lihor-hub/romanian-folk-fight/issues/132)
- [ ] [#135 Ranged weapons and reach](https://github.com/lihor-hub/romanian-folk-fight/issues/135)

Integration constraints:

- `#130`, `#131`, and `#135` use continuous world units; no new distance bands.
- `#131` and `#132` use #150 for penalties, buffs, locks, and expiry.
- Every action uses #143 for label, icon, cost, chance, legality, and disabled reason.
- Fixed-seed tests pin behavior before UI work begins.

### Issue sequence 4.3: Persistent resources

- [ ] Implement [#133](https://github.com/lihor-hub/romanian-folk-fight/issues/133).
- [ ] Implement [#139](https://github.com/lihor-hub/romanian-folk-fight/issues/139).
- [ ] Define `Untură de urs` once as armour-pool restoration in #139.
- [ ] Extend the snapshot through a new version and migration when #133 adds persistent current HP, consumable inventory, and allowed between-fight effects.
- [ ] Preserve current HP between ordinary fights after #133. Ordinary-fight stamina continues to refill under the existing fight-start rule.
- [ ] Refill current armour to equipment-derived maximum at the start of every fight after #139, including tournament rounds; armour damage does not persist between safe checkpoints.

### Wave 4 exit evidence

- [ ] All new actions render without HUD-specific layout branches.
- [ ] Reach, displacement, effects, armour, and persistence each have one authoritative model.
- [ ] Seeded simulations reproduce results and expose balance metrics for later #149.

## 11. Wave 5 — Full town campaign

Every Wave 5 issue is a separate worktree and PR. Merge and rebase in the listed order unless an issue's files are proven disjoint in its own approved implementation plan.

### Issue sequence 5.1: Town spine and locations

- [ ] Implement [#129](https://github.com/lihor-hub/romanian-folk-fight/issues/129).
- [ ] Implement [#147](https://github.com/lihor-hub/romanian-folk-fight/issues/147) after the town interaction zones are stable.
- [ ] Use the shared responsive screen shell and centralized flow intents.

### Issue sequence 5.2: Services

- [ ] Implement [#136](https://github.com/lihor-hub/romanian-folk-fight/issues/136) after spell and consumable catalogs exist.
- [ ] Instantiate one reusable storefront framework for fierărie, armurărie, coliba vrăjitoarei, and biserică/rest.
- [ ] A missing catalog produces a deliberate unavailable state, not an absent or broken destination.

### Issue sequence 5.3: Player-facing campaign sequence

- [ ] Implement [#137](https://github.com/lihor-hub/romanian-folk-fight/issues/137) using final actions, runtime assets, and snapshot flags.
- [ ] Implement [#138](https://github.com/lihor-hub/romanian-folk-fight/issues/138) from final combat/opponent view data.
- [ ] Implement [#140](https://github.com/lihor-hub/romanian-folk-fight/issues/140) last, persisting matchup and tournament state.
- [ ] Extend the snapshot with a new version/migration when #137 adds tutorial-completion state.
- [ ] Extend it again when #140 adds current matchup, tournament bracket/round, persistent HP, and current stamina between tournament rounds. Missing legacy stamina defaults to the normal maximum when migrating into a tournament-capable schema.

### Wave 5 exit evidence

- [ ] New run → tutorial → town → opponent preview → duel → recovery/shop → rematch/tournament is playable.
- [ ] Every safe checkpoint resumes at the correct destination after reload.
- [ ] Town and arena locations frame correctly at desktop, phone, ultrawide, 200% browser zoom, and reduced-motion settings.

## 12. Wave 6 — Balance and release proof

- [ ] Implement [#149](https://github.com/lihor-hub/romanian-folk-fight/issues/149).
- [ ] Simulate representative strength, agility, vitality, luck, charisma, magic, ranged, and armour builds across fixed seeds.
- [ ] Report win rate, turns, action usage, resource starvation, economy curve, item timing, healing, and tournament completion.
- [ ] Complete at least three structured human campaign runs from fresh browser profiles.
- [ ] Convert every release-blocking finding into a focused issue with reproduction and acceptance criteria.
- [ ] Re-run the full Rust, WASM, asset, browser, screenshot, accessibility, and campaign gates after fixes merge.

## 13. Parallel assignment rules

The program may use multiple agents only when their files and contracts are independent.

Safe examples:

- #142 flow and #152 bootstrap before either creates a dependency on the other's output.
- #141 asset tooling and #144 browser smoke after #152 merges, because their `xtask` modules and workflow files are explicitly disjoint.
- Separate roster-asset identities under #148 only when each agent owns a different source/runtime directory and a coordinator owns the shared manifest merge.

Unsafe examples:

- #116 and #117 editing the cutout spawn/hierarchy simultaneously.
- #134 and #135 defining reach at the same time.
- #130, #131, #132, or #135 running concurrently before a reviewed action-registration seam removes their shared exhaustive owners.
- #129, #136, and #140 independently adding state transitions before #142 merges.
- Multiple asset agents changing the same source sheet or runtime identity directory.

When two ready issues touch one owner file, run them sequentially or extract the shared owner in the earlier issue and rebase the later issue after merge.

## 14. Program completion checklist

- [ ] All tracker issues in #151 are closed by merged PRs with proof bundles.
- [ ] No open release-blocking visual, flow, asset, accessibility, save, or balance issue remains.
- [ ] The gold compact journey and full town journey pass from clean browser profiles.
- [ ] All runtime assets are manifest-backed, credited, and reviewed at game scale.
- [ ] All navigation uses flow intents; all run persistence uses versioned snapshots.
- [ ] All combat actions and timed effects use their shared contracts.
- [ ] `cargo fmt`, native clippy/tests, WASM clippy/build, asset validation, browser matrix, and screenshot diffs pass from a clean checkout.
- [ ] Structured human playtests report no release-blocking comprehension, pacing, readability, or fun problem.
