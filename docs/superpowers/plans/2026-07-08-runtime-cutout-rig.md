# Runtime Cutout Rig Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first runtime pixel-art cutout rig path for issue #89, shared by the creator preview and arena fighters.

**Architecture:** Add a focused `src/cutout.rs` module that owns rig templates, part metadata, and the body-part spawner. The arena and creator call the same helper so the player preview and fight scene prove one rendering path, while existing combat animation state stays on root fighter entities.

**Tech Stack:** Rust, Bevy ECS, existing headless `MinimalPlugins` tests, existing `GameState` scene plugins.

## Global Constraints

- Follow `docs/art-direction.md`: polished pixel-art cutout fighters, transparent parts, authored facing right, mirrored with `flip_x`.
- Keep current sprite-sheet assets available; do not delete placeholder sprite sheets or gear overlays in this slice.
- Do not slice generated source sheets in this PR; source sheets under `assets/fighters/*/source/` remain production-intent inputs for later cleanup.
- Rust gates must pass: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.

---

## File Structure

- Create `src/cutout.rs`: rig template data, marker components, placeholder sprite helper, and `spawn_cutout_rig`.
- Modify `src/lib.rs`: expose and register `CutoutRigPlugin`.
- Modify `src/arena/mod.rs`: spawn cutout rigs on player/enemy roots and keep root animation components.
- Modify `src/arena/animation.rs`: allow animation systems to advance root clips without requiring a root `Sprite` atlas.
- Modify `src/creation/mod.rs`: add a small creator preview entity that uses `spawn_cutout_rig`.

### Task 1: Add Cutout Rig Data And Spawner

**Files:**
- Create: `src/cutout.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Produces: `CutoutRigPlugin`, `CutoutTemplate`, `CutoutPartKind`, `CutoutRig`, `CutoutPartMarker`, `human_template()`, `enemy_template()`, `spawn_cutout_rig(commands: &mut Commands, root: Entity, template: CutoutTemplate, asset_server: Option<&AssetServer>, flip_x: bool)`.

- [ ] **Step 1: Write failing tests**

Add tests in `src/cutout.rs` that assert the human template contains torso, head, upper/lower arms, hands, upper/lower legs, and feet; spawning creates one marked child per part; and `flip_x` mirrors child X positions.

- [ ] **Step 2: Run red test**

Run: `cargo test cutout`

Expected: compile or test failure because `src/cutout.rs` and exported types do not exist yet.

- [ ] **Step 3: Implement minimal module**

Create `src/cutout.rs` with rig metadata structs, static human/enemy templates, placeholder colored sprites when no loaded image is needed, and child spawning with `CutoutPartMarker`.

- [ ] **Step 4: Export plugin**

Add `pub mod cutout;` in `src/lib.rs` and add `cutout::CutoutRigPlugin` to the app plugin list.

- [ ] **Step 5: Run green test**

Run: `cargo test cutout`

Expected: PASS.

### Task 2: Use The Rig In The Arena

**Files:**
- Modify: `src/arena/mod.rs`
- Modify: `src/arena/animation.rs`

**Interfaces:**
- Consumes: `spawn_cutout_rig`, `human_template`, `enemy_template`, `CutoutRig`.
- Produces: player and enemy arena fighter roots with `CutoutRig` children and existing `FighterClip` / `SpriteAnimation` state.

- [ ] **Step 1: Write failing arena tests**

Extend arena tests to assert the player and enemy roots each have `CutoutRig`; child markers exist under both; the enemy rig is mirrored; and combat events can still put fighters into `Attack`, `Hurt`, and `Ko` clips.

- [ ] **Step 2: Run red test**

Run: `cargo test arena::`

Expected: FAIL because arena fighters still rely on root atlas sprites only.

- [ ] **Step 3: Attach rigs to fighter roots**

Change `spawn_arena_fighter` to accept a cutout template and `flip_x`, insert animation state on the root, and call `spawn_cutout_rig` after `spawn_fighter`.

- [ ] **Step 4: Keep animation systems root-based**

Update animation queries so `FighterClip` and `SpriteAnimation` can advance even when a root fighter has no atlas `Sprite`. If an atlas sprite is present, keep the old atlas-index update behavior.

- [ ] **Step 5: Run green tests**

Run: `cargo test arena::`

Expected: PASS.

### Task 3: Add Creator Preview

**Files:**
- Modify: `src/creation/mod.rs`

**Interfaces:**
- Consumes: `spawn_cutout_rig`, `human_template`, `CutoutRig`.
- Produces: a creator-screen preview root using the same cutout renderer as the arena.

- [ ] **Step 1: Write failing creator test**

Extend `entering_creation_spawns_the_screen` or add a new test that enters `GameState::CharacterCreation` and asserts exactly one `CutoutRig` preview exists under the creation screen.

- [ ] **Step 2: Run red test**

Run: `cargo test creation::`

Expected: FAIL because the creator screen has no visual fighter preview.

- [ ] **Step 3: Spawn preview**

Add a compact preview node/entity near the existing character name/stat preview, then attach `human_template()` with `spawn_cutout_rig`.

- [ ] **Step 4: Run green tests**

Run: `cargo test creation::`

Expected: PASS.

### Task 4: Full Verification And PR

**Files:**
- Modify only files changed by Tasks 1-3.

**Interfaces:**
- Consumes: complete implementation.
- Produces: verified branch ready for PR #89.

- [ ] **Step 1: Run formatting check**

Run: `cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 3: Run full tests**

Run: `cargo test`

Expected: PASS.

- [ ] **Step 4: Review issue coverage**

Confirm #89 acceptance criteria are covered: player rig, enemy rig, explicit part transforms, creator and arena path, combat flow, and headless tests.

- [ ] **Step 5: Commit and publish**

Commit with `feat: render runtime cutout rigs`, push `codex/runtime-cutout-rig-89`, open a PR with `Closes #89`, and enable auto-merge after green CI.

## Self-Review

- Spec coverage: each #89 acceptance criterion maps to Tasks 1-3, with verification in Task 4.
- Placeholder scan: no deferred implementation placeholders remain; source-sheet slicing is explicitly out of scope.
- Type consistency: task interfaces use the same names as the design spec.
