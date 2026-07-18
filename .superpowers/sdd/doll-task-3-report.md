# Task 3 report — catalog-backed creation and generation

## Result

DONE. Character creation now owns and confirms the exact catalog-backed
`CharacterDefinition` shown in its preview. Hoț de codru generation uses the
complete authored body, face, hair, and correlated wardrobe pools without
changing encounter-seed derivation or save authority.

## Implementation

- Added ordered Romanian creator choices for `Trup`, `Chip`, `Păr`, and
  `Port`, alongside the existing four-choice `Piele` and `Accent` palette
  controls.
- `CharacterDraft` now owns its resolved definition. Body, face, hair, and
  wardrobe mutations validate a cloned candidate through the shared
  `CharacterCatalog` before committing it; confirmation clones that exact
  preview definition into `PlayerCharacter`.
- `WardrobeChoice::{Haiduc, Cioban}` changes torso, legs, and feet as one
  unit. Presets and reset use explicit authored IDs and never reconstruct
  through `legacy_human`.
- Added `WeightedWardrobe` and one correlated deterministic wardrobe draw.
  Profiles mixing a wardrobe pool with torso/legs/feet slots are rejected.
- Expanded Hoț de codru to both authored bodies, both authored faces, all
  three production hairs, and both complete wardrobes. Campaign seeds 0 and
  2 pin Haiduc and Cioban outcomes respectively.
- Kept `derive_encounter_seed`, save `CURRENT_VERSION` (5), and the saved
  `PreparedEncounter.definition` authority unchanged. No snapshot schema
  edit was needed; existing current/legacy migration and identity-roundtrip
  tests cover the changed generated definitions.

## TDD evidence

- RED: `cargo test --lib creation::draft` failed on missing
  `default_with_catalog`, `CreatorPartField`, `CycleDirection`, and
  `WardrobeChoice` APIs.
- RED: `cargo test --lib character::generation` failed on missing
  `WeightedWardrobe`, `GenerationProfile.wardrobes`, and conflict validation.
- RED: the focused creation ECS test failed on missing `Chip`/`Port` rows and
  ordered six-selector API.
- RED: the focused roster pool test reported only
  `human.body.foundation.v1` instead of the two authored body IDs.
- GREEN: draft 18/18, creation 41/41, generation 11/11, roster 24/24, and
  save snapshot 23/23.

## Verification

- `cargo test --lib creation::` — pass (41 tests)
- `cargo test --lib character::generation` — pass (11 tests)
- `cargo test --lib roster::` — pass (24 tests)
- `cargo test --lib save::snapshot::` — pass (23 tests)
- `cargo xtask test logic` — pass
- `cargo fmt --all -- --check` — pass
- `cargo clippy --all-targets -- -D warnings` — pass

## Self-review

Standards and spec were reviewed independently against `AGENTS.md`, the Rust
skill, and `.superpowers/sdd/task-3-brief.md`. One runtime enum-table
`.expect` was removed. The remaining static `PartId` conversions are vetted,
non-blank authored literals using the type's public checked constructor. The
creator's catalog setup inserts the bundled resource only for standalone
plugin apps; the production plugin order reuses `CharacterPlugin`'s existing
resource. No missing Task 3 requirement or unrelated scope expansion remains.
