# Romanian Paper-Doll Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Issue:** #323 (first bounded slice; this plan does not close the four-look issue)

**Goal:** Replace the temporary human tracer-bullet art with two production-intent Romanian paper-doll looks whose real stable part IDs can be selected in creation and recombined deterministically for a human NPC without changing identity between creation, shop, save/reload, and combat.

**Architecture:** Version the character catalog from one media attachment per semantic part to a semantic part containing one or more typed rig layers. Keep `CharacterDefinition` version 1 and the existing human cutout hierarchy authoritative; creator controls and seeded generation change resolved `PartId`s, while catalog layers provide the albedo and hybrid material inputs for every affected rig entity.

**Tech Stack:** Rust, Bevy ECS/UI, serde/serde_json, Bevy `Material2d`, WGSL, transparent PNG layers, cargo xtask asset validation/gallery, and Playwright-backed web smoke tests.

## Global Constraints

- This slice ships two authored looks: **Haiduc** and **Cioban**. The remaining two faces/hair identities and Voinic/Ucenic Solomonar wardrobe sets required to close #323 remain follow-up work.
- One `CharacterDefinition` and stable `PartId` set remains authoritative across creator preview, shop, save/reload, seeded generation, and arena rendering.
- Set `CHARACTER_CATALOG_VERSION` to `3`; keep `CHARACTER_DEFINITION_VERSION` at `1` because persisted selection fields and meanings do not change.
- Author every runtime layer facing right; renderer-owned `flip_x` remains the only mirroring mechanism.
- Every new fighter albedo has exact-dimension, exact-alpha mask, normal, and shadow companions; at most three RGB palette regions; nearest/pixel-consistent sampling; documented pivot, display size, attachment, provenance, cultural reference, and rights.
- Use the established Romanian grammar: cream linen, restrained red/black geometric embroidery, ițari or cioareci, opinci, cojoc/căciulă, and culturally compatible bâtă/topor equipment. Do not introduce generic fantasy plate, unrestricted cultural mixing, smoothing, or runtime image generation.
- Keep the catalog-owned known-good human and the pre-catalog sprite fallback operational. Do not add non-human skeletons, named-opponent conversion, new combat rules, or remove legacy fallbacks.
- Do not add `CutoutPartKind` variants in this slice. Facial hair, waist/apron/fotă layers, and arbitrary accessories remain excluded until their renderer contract is designed.
- Catalog albedo alpha is the silhouette authority. Do not stack legacy `BodyBuild` width scaling or `HairStyle` size/offset transforms on the new catalog layers; keep the existing joint hierarchy/rest pose and vary shape inside its authored display boxes.
- Rebase onto `origin/main` before capturing visual baselines. Run `cargo xtask pre-push` before any push; never use `--no-verify`.

## File Structure

- `src/character/catalog.rs`: v3 `PartLayerRecord`, layer-set validation, stable lookup, and known-good fallback.
- `src/character/generation.rs`: deterministic atomic wardrobe selection plus weighted body/face/hair selection.
- `src/creation/draft.rs`: a catalog-backed resolved definition and atomic creator mutations.
- `src/creation/mod.rs`: stable-ID body, face, hair, wardrobe, skin, and cloth-palette controls.
- `src/cutout.rs`: maps every selected catalog layer onto the existing human rig without changing transforms or poses.
- `assets/fighters/catalog/human-foundation.json`: two bodies, two faces, three hair silhouettes, two coherent wardrobe sets, and the known-good selection.
- `assets/fighters/human/runtime/{shared,haiduc,cioban}/`: production fighter layers and hybrid companions.
- `assets/fighters/gear/runtime/`: production replacements for the five equipment layers used by the two looks.
- `xtask/src/assets/validate/catalog.rs`: catalog-layer/media cross-validation.
- `xtask/src/assets/gallery/{model,mod}.rs`: composed Haiduc and Cioban review specimens in both facings.
- `src/review/mod.rs` and `xtask/src/web_smoke/romanian_paper_doll_library.rs`: cross-scene identity telemetry and browser proof.

---

### Task 1: Version the catalog for layered semantic bundles

**Files:**
- Modify: `src/character/catalog.rs`
- Modify: `src/character/mod.rs`
- Modify: `src/character/material.rs`
- Modify: `src/cutout.rs`
- Modify: `assets/fighters/catalog/human-foundation.json`
- Modify: `xtask/src/assets/validate/catalog.rs`
- Modify: `xtask/src/assets/gallery/model.rs`

**Interfaces:**
- Produces: `PartLayerRecord { asset_path: String, attachment: AttachmentMetadata, material: MaterialMetadata }`.
- Produces: `PartRecord.layers: Vec<PartLayerRecord>` and `PartRecord::layer_for_attachment(&str) -> Option<&PartLayerRecord>`.
- Produces: `required_attachment_points(BodyRegion) -> &'static [&'static str]` for `body`, `face`, `hair`, `torso`, `legs`, and `feet`.
- Consumes: existing `PartId`, `BodyRegion`, `ResolvedCharacter`, `CutoutPartKind`, and `resolve_material_for_part` semantics.
- Preserves: `CharacterDefinition`, `PartSelections`, existing pivots/rest transforms, and `CutoutPartMarker.source_id`.
- Produces: `catalog_human_template(PlayerAppearance) -> CutoutRigTemplate`, which applies palette colors to the neutral human rest rig without legacy build/hair geometry transforms.

- [ ] **Step 1: Write failing v3 catalog tests**

In `src/character/catalog.rs`, add fixtures/tests proving v2 is rejected, required layer sets are complete, attachment points are unique, every layer is compatible with its semantic region, and a known-good v3 definition resolves all 15 human rig attachments.

```rust
#[test]
fn body_bundle_requires_both_complete_arm_chains() {
    let catalog = fixture_with_layer_removed(
        "human.body.foundation.v1",
        "forearm_back",
    );
    assert_eq!(
        catalog.validate(),
        Err(CatalogError::MissingPartLayer {
            part_id: id("human.body.foundation.v1"),
            attachment: "forearm_back".to_owned(),
        })
    );
}

#[test]
fn known_good_human_resolves_every_cutout_attachment() {
    let resolved = catalog().resolve_known_good_human().unwrap();
    let attachments = resolved
        .parts()
        .values()
        .flat_map(|part| part.layers.iter())
        .map(|layer| layer.attachment.point.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(attachments, EXPECTED_HUMAN_ATTACHMENTS.into_iter().collect());
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test --lib character::catalog
```

Expected: compilation fails because `PartLayerRecord`, `PartRecord.layers`, and the new layer errors do not exist.

- [ ] **Step 3: Implement the v3 typed layer schema and migration**

Move `asset_path`, `attachment`, and `material` from `PartRecord` into a denied-unknown-fields layer record, set the catalog version to 3, and require these exact attachment sets:

```rust
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartLayerRecord {
    pub asset_path: String,
    pub attachment: AttachmentMetadata,
    #[serde(default)]
    pub material: MaterialMetadata,
}

pub const CHARACTER_CATALOG_VERSION: u32 = 3;

fn required_attachment_points(region: BodyRegion) -> &'static [&'static str] {
    match region {
        BodyRegion::Body => &[
            "upper_arm_back", "forearm_back", "hand_back",
            "upper_arm_front", "forearm_front", "hand_front",
        ],
        BodyRegion::Face => &["head"],
        BodyRegion::Hair => &["hair"],
        BodyRegion::Torso => &["torso"],
        BodyRegion::Legs => &["thigh_back", "shin_back", "thigh_front", "shin_front"],
        BodyRegion::Feet => &["foot_back", "foot_front"],
        BodyRegion::FacialHair | BodyRegion::Waist | BodyRegion::Accessory => &[],
    }
}
```

Add explicit `CatalogError::MissingPartLayer`, `DuplicatePartLayer`, and `UnexpectedPartLayer` variants. Migrate every existing JSON record to `layers`, including one complete layer bundle for the current known-good records; do not change the known-good stable IDs.

- [ ] **Step 4: Make the cutout adapter consume every selected layer**

In `src/cutout.rs`, replace the single-attachment conditional with lookup by the live rig kind. The selected semantic record still supplies one `source_id` to every member of its bundle.

```rust
if let Some(record) = selected_record_for_kind(character, part.kind) {
    part.source_id = Some(record.id.clone());
    if let Some(layer) = record.layer_for_attachment(attachment_name(part.kind)) {
        part.asset_path = Some(layer.asset_path.clone());
        part.material = Some(resolve_material_for_layer(&record.id, layer));
    }
}
```

Build this adapter from `catalog_human_template(character.definition().appearance)`, not `human_template_for`: the latter's `apply_build` and `apply_hair_style` transforms are legacy projection behavior and would distort authored body/hair silhouettes.

Update `src/character/material.rs` as needed so `resolve_material_for_layer(part_id: &PartId, layer: &PartLayerRecord) -> ResolvedPartMaterial` preserves stable-ID provenance while reading layer-owned material metadata.

- [ ] **Step 5: Extend asset validation and prove GREEN**

Make `xtask/src/assets/validate/catalog.rs` iterate every catalog layer and require a registered runtime image with matching attachment, pivot, dimensions, alpha, and material companions. Add xtask fixture tests for a missing layer asset and mismatched pivot.

Run:

```bash
cargo test --lib character::catalog character::material cutout::
cargo test -p xtask assets::validate
cargo xtask assets check
```

Expected: all focused tests pass; the bundled v3 catalog resolves all 15 human parts and asset validation reports no orphaned or mismatched layer references.

- [ ] **Step 6: Commit checkpoint**

```bash
git add src/character src/cutout.rs assets/fighters/catalog/human-foundation.json xtask/src/assets
git commit -m "feat: support layered character part bundles"
```

### Task 2: Author the two-look Romanian production library

**Files:**
- Create: `assets/fighters/human/source/romanian-paper-doll-v1/manifest.toml`
- Create: `assets/fighters/human/source/romanian-paper-doll-v1/README.md`
- Create: `assets/fighters/human/source/romanian-paper-doll-v1/romanian-paper-doll-v1.png`
- Create: `assets/fighters/human/runtime/shared/manifest.toml`
- Create: `assets/fighters/human/runtime/haiduc/manifest.toml`
- Create: `assets/fighters/human/runtime/cioban/manifest.toml`
- Modify: `assets/fighters/gear/runtime/manifest.toml`
- Modify: `assets/fighters/catalog/human-foundation.json`
- Modify: `assets/CREDITS.md`
- Modify: `docs/art-direction.md`
- Modify: `xtask/src/assets/gallery/mod.rs`
- Modify: `xtask/src/assets/gallery/model.rs`

**Interfaces:**
- Produces body IDs: `human.body.zvelt.v1`, `human.body.vanjos.v1`.
- Produces face IDs: `human.face.haiduc.v1`, `human.face.cioban.v1`.
- Produces hair IDs: `human.hair.plete.v1`, `human.hair.prins.v1`, `human.hair.scurt.v1`.
- Produces wardrobe IDs: `human.torso.ie_altita.v1`, `human.legs.itari.v1`, `human.torso.camasa_ciobaneasca.v1`, `human.legs.cioareci.v1`, and shared `human.feet.opinci.v1`.
- Produces production equipment art for existing `ItemId::{ToporDePadurar, BataCiobaneasca, CojocGros, CaciulaDeOaie, OpinciIuti}` without changing gameplay stats or IDs.
- Consumes: Task 1's v3 catalog layers and existing hybrid material authoring contract.

- [ ] **Step 1: Add failing catalog identity and gallery tests**

Add tests in `src/character/catalog.rs` that load the exact stable IDs above, validate each layer bundle, and resolve these two complete selections:

```rust
const HAIDUC_LOOK: [&str; 6] = [
    "human.body.zvelt.v1",
    "human.face.haiduc.v1",
    "human.hair.plete.v1",
    "human.torso.ie_altita.v1",
    "human.legs.itari.v1",
    "human.feet.opinci.v1",
];

const CIOBAN_LOOK: [&str; 6] = [
    "human.body.vanjos.v1",
    "human.face.cioban.v1",
    "human.hair.prins.v1",
    "human.torso.camasa_ciobaneasca.v1",
    "human.legs.cioareci.v1",
    "human.feet.opinci.v1",
];
```

In `xtask/src/assets/gallery/model.rs`, add failing model tests requiring `composition.human.haiduc` and `composition.human.cioban`, each with normal and mirrored specimens and the exact resolved IDs.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test --lib character::catalog
cargo test -p xtask assets::gallery
```

Expected: tests fail because the production IDs, media records, and gallery compositions do not exist.

- [ ] **Step 3: Produce and register the exact runtime layer sets**

Create albedo plus `_mask.png`, `_normal.png`, and `_shadow.png` companions for these exact albedo stems:

```text
runtime/shared/foot_back, foot_front
runtime/haiduc/upper_arm_back, forearm_back, hand_back,
  upper_arm_front, forearm_front, hand_front, head, hair, torso,
  thigh_back, shin_back, thigh_front, shin_front
runtime/cioban/upper_arm_back, forearm_back, hand_back,
  upper_arm_front, forearm_front, hand_front, head, hair, torso,
  thigh_back, shin_back, thigh_front, shin_front
runtime/shared/hair_scurt
```

Each listed stem represents four tracked PNGs. Use RGB masks only for skin, hair, cloth, embroidery, and leather; preserve exact albedo alpha in all companions. Register every file in its local `manifest.toml`, then add v3 catalog records/layers with `romanian` and specific `haiduc`, `cioban`, `ie`, `itari`, `cioareci`, or `opinci` tags.

Replace the five existing equipment albedos in place under `assets/fighters/gear/runtime/`, keeping their current attachment points and `ItemId` mappings. Update their manifest dimensions/pivots if the curated trim changes; do not change `src/items/catalog.rs` stats.

- [ ] **Step 4: Record the cultural and rights contract**

Document in `README.md`, manifests, `assets/CREDITS.md`, and `docs/art-direction.md`:

- the visual references used for ie/altiță, ițari, cioareci, opinci, cojoc, and căciulă;
- the source master and rights provenance;
- right-facing orientation, outline weight, pixel density, upper-left light, RGB mask meaning, and pivot rules;
- that these are a pan-Romanian folkloric remix, not a claim of one exact historical regional costume.

- [ ] **Step 5: Validate and review both looks**

Run:

```bash
cargo xtask assets check
cargo xtask assets review
cargo test --lib character::catalog character::material cutout::
```

Expected: validation passes; the printed gallery contains normal/mirrored Haiduc and Cioban compositions; both remain readable at 192–256px, have clean joint overlap, and preserve crisp motifs under the hybrid material.

- [ ] **Step 6: Commit checkpoint**

```bash
git add assets/fighters assets/CREDITS.md docs/art-direction.md xtask/src/assets/gallery
git commit -m "feat: add two Romanian paper-doll looks"
```

### Task 3: Drive catalog IDs from creation and deterministic generation

**Files:**
- Modify: `src/creation/draft.rs`
- Modify: `src/creation/mod.rs`
- Modify: `src/character/generation.rs`
- Modify: `src/roster/mod.rs`
- Modify: `src/save/snapshot.rs`

**Interfaces:**
- Produces: `CreatorPartField::{Body, Face, Hair, Wardrobe}`.
- Produces: `WardrobeChoice::{Haiduc, Cioban}` with `apply(&mut PartSelections)` changing torso, legs, and feet atomically.
- Produces: `CharacterDraft::definition(&self) -> &CharacterDefinition`, `cycle_part(field, direction, &CharacterCatalog)`, and `wardrobe() -> WardrobeChoice`.
- Produces: `WeightedWardrobe { torso: PartId, legs: PartId, feet: PartId, weight: u32 }` and `GenerationProfile.wardrobes` for correlated seeded selection.
- Consumes: the same `CharacterCatalog` resource and stable IDs used by rendering; keeps `PlayerAppearance.skin_tone` and `.accent` as the four existing hybrid palette controls.

- [ ] **Step 1: Write failing pure draft and generation tests**

In `src/creation/draft.rs`, prove creator cycling changes actual stable IDs, wardrobe changes three IDs atomically, and preset/reset operations never reconstruct through `legacy_human`.

```rust
#[test]
fn cycling_face_changes_the_resolved_part_id() {
    let catalog = catalog();
    let mut draft = CharacterDraft::default_with_catalog(&catalog).unwrap();
    let before = draft.definition().parts.face.clone();
    draft.cycle_part(CreatorPartField::Face, CycleDirection::Next, &catalog).unwrap();
    assert_ne!(draft.definition().parts.face, before);
}

#[test]
fn cioban_wardrobe_changes_a_complete_compatible_set() {
    let catalog = catalog();
    let mut draft = CharacterDraft::default_with_catalog(&catalog).unwrap();
    draft.select_wardrobe(WardrobeChoice::Cioban, &catalog).unwrap();
    assert_eq!(draft.definition().parts.torso.as_str(), "human.torso.camasa_ciobaneasca.v1");
    assert_eq!(draft.definition().parts.legs.as_str(), "human.legs.cioareci.v1");
    assert_eq!(draft.definition().parts.feet.as_str(), "human.feet.opinci.v1");
}
```

In `src/character/generation.rs`, pin one seed to Haiduc and another to Cioban, assert identical seed/profile/catalog bytes are identical, and assert no cross-paired torso/legs wardrobe is generated.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test --lib creation::draft character::generation roster::
```

Expected: compilation fails because the catalog-backed draft and atomic wardrobe pool do not exist.

- [ ] **Step 3: Make `CharacterDraft` own the resolved identity**

Store `definition: CharacterDefinition` in the draft and mutate its IDs directly. Keep `appearance` only through `definition.appearance`; confirmation must clone the exact previewed definition into `PlayerCharacter`.

```rust
pub struct CharacterDraft {
    choice: HeroChoice,
    custom_name_index: usize,
    attributes: Attributes,
    definition: CharacterDefinition,
    wardrobe: WardrobeChoice,
}

pub fn definition(&self) -> &CharacterDefinition {
    &self.definition
}
```

Use explicit ordered option tables for this bounded library; labels and IDs must be exact and Romanian-localized. `WardrobeChoice::apply` must update torso/legs/feet, then call `catalog.resolve` before committing the mutation so an invalid draft never reaches the preview.

- [ ] **Step 4: Wire six creator selectors without losing palette controls**

In `src/creation/mod.rs`, render these rows in order: `Piele`, `Trup`, `Chip`, `Păr`, `Port`, `Accent`. Extend the existing previous/next action pattern; `Trup`, `Chip`, `Păr`, and `Port` call catalog-backed draft methods, while `Piele` and `Accent` retain the four hybrid palette choices. Refresh the preview from `draft.definition()` and update tab-order/touch-target/phone layout tests from four to six selector rows.

Add an ECS test that presses each new selector and compares `CutoutPartMarker.source_id` values before/after rather than checking labels alone.

- [ ] **Step 5: Add correlated seeded wardrobe generation**

Extend `GenerationProfile` with a weighted wardrobe pool selected by one deterministic draw before optional slots. Reject a profile that supplies both a wardrobe pool and individual torso/legs/feet slots.

```rust
pub struct WeightedWardrobe {
    pub torso: PartId,
    pub legs: PartId,
    pub feet: PartId,
    pub weight: u32,
}
```

Update `hot_de_codru_profile` to use both body IDs, both face IDs, all three hair IDs, and the two exact wardrobe choices. Keep `derive_encounter_seed` unchanged. Loaded `PreparedEncounter.definition` remains authoritative and must not regenerate after catalog load.

- [ ] **Step 6: Verify creation, generation, and persistence**

Run:

```bash
cargo test --lib creation:: character::generation roster:: save::snapshot::
cargo xtask test logic
```

Expected: direct selectors visibly change resolved IDs; wardrobe selection is atomic; golden seeds produce valid distinct looks; current and legacy saves round-trip without identity drift.

- [ ] **Step 7: Commit checkpoint**

```bash
git add src/creation src/character/generation.rs src/roster/mod.rs src/save/snapshot.rs
git commit -m "feat: configure and generate Romanian doll identities"
```

### Task 4: Prove identical identities across every scene and browser viewport

**Files:**
- Modify: `src/review/mod.rs`
- Create: `xtask/src/web_smoke/romanian_paper_doll_library.rs`
- Modify: `xtask/src/web_smoke/mod.rs`
- Modify: `xtask/src/commands/web_smoke_cmd.rs`
- Create: `tests/visual/baselines/romanian-paper-doll-library/desktop-haiduc.png`
- Create: `tests/visual/baselines/romanian-paper-doll-library/desktop-cioban.png`
- Create: `tests/visual/baselines/romanian-paper-doll-library/phone-haiduc.png`
- Create: `tests/visual/baselines/romanian-paper-doll-library/phone-cioban.png`

**Interfaces:**
- Produces: review snapshot `rff_review_paper_doll_v1` with `creation`, `shop`, `reloaded`, `combat_player`, and `combat_npc` identity facts.
- Each identity fact contains: `seed`, semantic-order `resolved_part_ids`, `rig_source_ids`, `part_count`, `hybrid_part_count`, and `fallback_part_count`.
- Consumes: existing review-only browser bridge, save/reload seam, creation actions, prepared encounter telemetry, and hybrid material telemetry.

- [ ] **Step 1: Write failing cross-scene ECS telemetry tests**

In `src/review/mod.rs`, build a Haiduc definition, save/reload it, and spawn it through creation/shop/arena adapters. Require exact semantic IDs and 15 source-ID-bearing rig parts in every scene. Add a second test for Cioban and one seeded `Hot de codru` NPC.

```rust
assert_eq!(snapshot.creation.resolved_part_ids, snapshot.shop.resolved_part_ids);
assert_eq!(snapshot.creation.resolved_part_ids, snapshot.reloaded.resolved_part_ids);
assert_eq!(snapshot.creation.resolved_part_ids, snapshot.combat_player.resolved_part_ids);
assert_eq!(snapshot.creation.part_count, 15);
assert_eq!(snapshot.creation.fallback_part_count, 0);
assert_ne!(snapshot.creation.resolved_part_ids, snapshot.combat_npc.resolved_part_ids);
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test --features review --lib review::
```

Expected: compilation/test failure because paper-doll review telemetry and cross-scene assertions do not exist.

- [ ] **Step 3: Implement review telemetry and the focused browser journey**

Expose `rff_review_paper_doll_v1` only under the `review` feature. Implement the new browser scenario to:

1. Create Haiduc by direct selectors and capture creation.
2. Continue to shop, save/reload, and fight; assert exact identity equality at every transition.
3. Return to creation, select Cioban, and repeat the identity assertions.
4. Enter the seeded Hot de codru encounter and assert its published seed and IDs match the preview and live combat rig.
5. Run the same journey at 1440×900 desktop and the existing phone viewport, requiring zero fallback parts and nonzero hybrid parts.

The four baselines capture the two creator looks at desktop and phone; telemetry, not screenshot similarity, proves shop/reload/combat identity.

- [ ] **Step 4: Rebase, capture, and inspect visual baselines**

Rebase before capture, then run:

```bash
git fetch origin
git rebase origin/main
cargo xtask web-smoke --scenario romanian-paper-doll-library --strict-visual
```

Expected: the scenario passes after the four reviewed baselines are accepted; both silhouettes, faces, hair, wardrobe, equipment overlap, embroidery, and phone readability are visibly distinct and intentional.

- [ ] **Step 5: Run the broader regression and repository gate**

Run:

```bash
cargo xtask web-smoke --scenario gold-journey
cargo xtask pre-push
git diff --check
```

Expected: gold journey passes; format, clippy, tests, default/review build matrix, asset checks, and whitespace checks all pass.

- [ ] **Step 6: Commit checkpoint**

```bash
git add src/review/mod.rs xtask/src/web_smoke xtask/src/commands/web_smoke_cmd.rs tests/visual/baselines/romanian-paper-doll-library
git commit -m "test: prove Romanian paper-doll identities end to end"
```

## Excluded Follow-Up Work for #323

- Add the Voinic and Ucenic Solomonar face, hair/beard, and wardrobe sets so #323 reaches four complete authored presets.
- Add explicit `FacialHair`, `Waist`, and accessory rig layers before selecting mustaches, beards, chimir/brâu, catrință, or fotă as independent saved parts.
- Convert named human encounters, non-human skeletons, or bosses.
- Apply the hybrid material to gear overlays or replace the remaining eight starter equipment assets.
- Retire the known-good and legacy sprite fallbacks.
