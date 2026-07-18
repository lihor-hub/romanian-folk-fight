# Modular Character Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver one data-driven human character definition that works for direct player customization, deterministic NPC generation, save/load, previews, and combat through the existing cutout rig.

**Architecture:** Add a focused character-definition model, a validated runtime catalog, and a seeded generator under `src/character/`. Adapt the existing cutout spawner to resolve a definition into its current rig representation, preserving pose and gear behavior while later phases add richer assets and the 2.5D material.

**Tech Stack:** Rust, Bevy ECS, serde/serde_json, existing cutout rig, existing snapshot system, cargo xtask asset validation.

## Global Constraints

- One definition format serves players, generated NPCs, and named encounters.
- Runtime generation is deterministic for a fixed seed, profile, and catalog version.
- Resolved stable part IDs are persisted; saves do not silently regenerate identities.
- Romanian cultural profile tags constrain wardrobe combinations.
- Existing combat rules, poses, equipment behavior, and full-frame fallback remain unchanged.
- This foundation does not add full 3D models, custom shaders, or broad new art production.

---

## File Structure

- `src/character/definition.rs`: stable IDs, skeleton family, cultural profile, selected parts, palette, and resolved character definition.
- `src/character/catalog.rs`: catalog records, compatibility checks, known-good fallback, and the first human records.
- `src/character/generation.rs`: deterministic selection from a generation profile and seed.
- `src/character/mod.rs`: exports and plugin/resource registration only.
- `src/cutout.rs`: compatibility adapter from `CharacterDefinition` to the existing `CutoutRigTemplate`.
- `src/creation/draft.rs`: edits one definition draft while retaining current presets and attributes.
- `src/creation/mod.rs`: uses the definition for preview and confirmation.
- `src/roster/mod.rs`: gives one representative NPC a seeded definition.
- `src/save/snapshot.rs`: versioned persistence and migration from `PlayerAppearance`.
- `assets/fighters/catalog/human-foundation.json`: first runtime-readable human catalog slice.
- `assets/fighters/catalog/manifest.toml`: provenance and asset-tool registration.

### Task 1: Define the stable character model

**Files:**
- Create: `src/character/definition.rs`
- Modify: `src/character/mod.rs:1-15`

**Interfaces:**
- Produces: `PartId`, `SkeletonFamily`, `CulturalProfile`, `PartSelections`, `CharacterDefinition`, and `CharacterDefinition::legacy_human(PlayerAppearance)`.
- Consumes: existing `PlayerAppearance` only for migration.

- [ ] **Step 1: Write failing model tests**

Add tests proving stable IDs reject blank values, a legacy appearance maps to the human skeleton, and JSON round-trips preserve every selected part.

```rust
#[test]
fn definition_round_trip_preserves_resolved_ids() {
    let definition = CharacterDefinition::legacy_human(PlayerAppearance::default());
    let json = serde_json::to_string(&definition).unwrap();
    assert_eq!(serde_json::from_str::<CharacterDefinition>(&json).unwrap(), definition);
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test --lib character::definition`

Expected: compilation fails because `character::definition` does not exist.

- [ ] **Step 3: Implement the minimal model**

Use serde-friendly owned IDs and an explicit schema version:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkeletonFamily { Human }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterDefinition {
    pub version: u32,
    pub seed: Option<u64>,
    pub skeleton: SkeletonFamily,
    pub culture: CulturalProfile,
    pub parts: PartSelections,
    pub appearance: PlayerAppearance,
}
```

Keep `PlayerAppearance` during this tracer bullet as the palette/proportion compatibility bridge; do not delete its save or UI contract.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test --lib character::definition`

Expected: all definition tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/character/definition.rs src/character/mod.rs
git commit -m "feat: define modular character identities"
```

### Task 2: Load and validate the first human catalog

**Files:**
- Create: `src/character/catalog.rs`
- Create: `assets/fighters/catalog/human-foundation.json`
- Create: `assets/fighters/catalog/manifest.toml`
- Modify: `src/character/mod.rs`
- Modify: `assets/manifest.toml`

**Interfaces:**
- Consumes: `PartId`, `SkeletonFamily`, and `PartSelections` from Task 1.
- Produces: `CharacterCatalog::from_json`, `CharacterCatalog::validate`, `CharacterCatalog::resolve`, and `CharacterCatalog::known_good_human`.

- [ ] **Step 1: Write failing catalog tests**

Cover duplicate IDs, missing required human regions, incompatible skeleton tags, missing companion parts, and successful resolution of the known-good human.

```rust
#[test]
fn human_catalog_rejects_a_missing_torso() {
    let catalog = fixture_without("human.torso.linen.v1");
    assert_eq!(catalog.validate(), Err(CatalogError::MissingRequiredRegion(BodyRegion::Torso)));
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test --lib character::catalog`

Expected: compilation fails because the catalog API is absent.

- [ ] **Step 3: Implement the registry and JSON records**

Each `PartRecord` must carry stable ID, body region, asset path, skeleton compatibility, cultural tags, attachment metadata, and exclusions. The first JSON catalog references the existing human runtime PNGs; no new fighter artwork is introduced.

```rust
pub struct CharacterCatalog {
    version: u32,
    parts: HashMap<PartId, PartRecord>,
    known_good_human: PartSelections,
}

pub fn resolve(&self, definition: &CharacterDefinition)
    -> Result<ResolvedCharacter, CatalogError>;
```

- [ ] **Step 4: Verify catalog and asset metadata**

Run: `cargo test --lib character::catalog && cargo xtask assets check`

Expected: catalog tests pass and asset validation reports no missing or duplicate records.

- [ ] **Step 5: Commit**

```bash
git add src/character assets/fighters/catalog assets/manifest.toml
git commit -m "feat: register the human character catalog"
```

### Task 3: Generate deterministic human NPC definitions

**Files:**
- Create: `src/character/generation.rs`
- Modify: `src/character/mod.rs`

**Interfaces:**
- Consumes: `CharacterCatalog` and its compatible candidate queries.
- Produces: `GenerationProfile`, `GenerationError`, and `generate_character(seed: u64, profile: &GenerationProfile, catalog: &CharacterCatalog) -> Result<CharacterDefinition, GenerationError>`.

- [ ] **Step 1: Write failing deterministic-generation tests**

Prove identical inputs produce byte-for-byte equal definitions, different seeds vary at least one unlocked choice, locked signature parts never change, and incompatible cultural tags are never selected.

The initial profile locks only parts the foundation renderer actually projects.
Optional waist identity remains absent until a dedicated waist layer is wired;
tests assert that the representative profile does not claim an invisible
chimir selection.

- [ ] **Step 2: Verify RED**

Run: `cargo test --lib character::generation`

Expected: compilation fails because generation is absent.

- [ ] **Step 3: Implement a dependency-free seeded selector**

Use a small private deterministic PRNG whose algorithm is pinned by golden tests; sort candidates by stable ID before weighted selection so `HashMap` iteration cannot affect output.

```rust
pub fn generate_character(
    seed: u64,
    profile: &GenerationProfile,
    catalog: &CharacterCatalog,
) -> Result<CharacterDefinition, GenerationError>;
```

Return `known_good_human()` only through an explicit fallback adapter; do not hide catalog errors inside the pure generator.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test --lib character::generation`

Expected: deterministic, cultural-compatibility, and identity-lock tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/character/generation.rs src/character/mod.rs
git commit -m "feat: generate deterministic modular fighters"
```

### Task 4: Persist resolved player identities

**Files:**
- Modify: `src/creation/mod.rs:758-770`
- Modify: `src/creation/draft.rs`
- Modify: `src/save/snapshot.rs:384-590`
- Test: existing inline tests in those modules.

**Interfaces:**
- Consumes: `CharacterDefinition::legacy_human` and stable resolved part IDs.
- Produces: `PlayerCharacter.definition: CharacterDefinition` and a save migration that reconstructs legacy definitions from `appearance`.

- [ ] **Step 1: Write failing save and creation tests**

Add a current-version round trip with non-default part IDs, a legacy save migration, and a confirmation test asserting that the previewed definition becomes the saved player definition.

- [ ] **Step 2: Verify RED**

Run: `cargo test --lib creation:: save::snapshot::`

Expected: tests fail because `PlayerCharacter` has no definition field.

- [ ] **Step 3: Add the definition without breaking old saves**

Keep `appearance` during migration and add the resolved definition with serde defaulting at the snapshot boundary. Increment the snapshot version using the migration pattern documented at the top of `src/save/snapshot.rs`.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test --lib creation:: save::snapshot::`

Expected: current and legacy round trips pass; character identity remains unchanged.

- [ ] **Step 5: Commit**

```bash
git add src/creation src/save/snapshot.rs
git commit -m "feat: persist resolved modular characters"
```

### Task 5: Render definitions through the existing cutout rig

**Files:**
- Modify: `src/cutout.rs:230-540`
- Modify: `src/arena/mod.rs`
- Modify: `src/creation/mod.rs`
- Modify: `src/shop/mod.rs`

**Interfaces:**
- Consumes: `ResolvedCharacter` from `CharacterCatalog::resolve`.
- Produces: `rig_template_for(&ResolvedCharacter) -> CutoutRigTemplate` and `spawn_character_rig(...)` as the shared preview/combat entry point.

- [ ] **Step 1: Write failing shared-renderer tests**

Assert the same definition produces the same part IDs and transforms in creation and arena roots, mirroring changes only facing, and existing gear attaches to the same semantic regions.

- [ ] **Step 2: Verify RED**

Run: `cargo test --lib cutout:: creation:: arena::`

Expected: tests fail because scenes still select templates directly.

- [ ] **Step 3: Add the compatibility adapter**

Resolve catalog part records into existing `CutoutPart` values and keep `CutoutPartKind` as the animation/attachment semantic region for this phase. Replace direct human-template selection at the creation, shop, and arena call sites with the shared definition adapter.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test --lib cutout:: creation:: arena:: shop::`

Expected: shared identity, pose, mirroring, and equipment tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/cutout.rs src/arena/mod.rs src/creation/mod.rs src/shop/mod.rs
git commit -m "feat: render modular definitions through cutout rigs"
```

### Task 6: Prove one seeded NPC vertical slice

**Files:**
- Modify: `src/roster/mod.rs`
- Modify: `src/arena/mod.rs`
- Modify: `src/review/mod.rs`

**Interfaces:**
- Consumes: `generate_character` and `spawn_character_rig`.
- Produces: one stable seeded human encounter definition and review output exposing its seed and resolved IDs.

- [ ] **Step 1: Write the failing arena identity test**

Enter the same encounter twice and assert equal generated definitions; enter it with a different explicit review seed and assert only unlocked choices may vary.

- [ ] **Step 2: Verify RED**

Run: `cargo test --lib roster:: arena:: review::`

Expected: the roster has no generated character definition.

- [ ] **Step 3: Wire one human encounter end to end**

Use a stable seed derived from encounter identity plus campaign seed. Keep every other opponent on the current template path. Expose resolved IDs in the review snapshot for browser assertions.

- [ ] **Step 4: Run focused and browser verification**

Run: `cargo test --lib roster:: arena:: review:: && cargo xtask web-smoke --scenario gold-journey`

Expected: focused tests pass; the gold journey shows the same identity in preview and combat at 1440×900 DPR 1.

- [ ] **Step 5: Commit**

```bash
git add src/roster/mod.rs src/arena/mod.rs src/review/mod.rs
git commit -m "feat: generate one seeded human encounter"
```

### Task 7: Run the repository gate and record the next-phase handoff

**Files:**
- Modify: `docs/art-direction.md`
- Modify: `xtask/README.md`

**Interfaces:**
- Consumes: the completed human tracer bullet.
- Produces: documented catalog authoring/validation workflow and the stable interfaces required by the 2.5D material and wardrobe-production phases.

- [ ] **Step 1: Document the operator workflow**

Document how to add a catalog part, assign cultural tags, validate it, inspect generated definitions, and invoke the known-good fallback during development.

- [ ] **Step 2: Run documentation checks**

Run: `git diff --check && cargo xtask assets check`

Expected: no whitespace errors; all catalog records and referenced assets validate.

- [ ] **Step 3: Run the required pre-push gate**

Run: `cargo xtask pre-push`

Expected: format, clippy, tests, and build matrix pass for default and review features.

- [ ] **Step 4: Commit**

```bash
git add docs/art-direction.md xtask/README.md
git commit -m "docs: explain modular character authoring"
```

## Subsequent Plans

After this foundation merges, write separate implementation plans for:

1. Hybrid 2.5D character material and rendering fallbacks.
2. Romanian wardrobe, face, hair, facial-hair, and embroidery library.
3. Named human encounter conversion.
4. Non-human skeleton families and folklore encounter conversion.
5. Boss and multi-headed anatomy support.
6. Legacy full-frame fighter retirement after parity.
