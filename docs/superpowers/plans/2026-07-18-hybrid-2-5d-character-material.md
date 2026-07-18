# Hybrid 2.5D Character Material Tracer Bullet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Issue:** #321
**Goal:** Give the existing Romanian human cutout one configurable, pixel-crisp 2.5D material treatment in creation, shop, and combat without changing character identity, rig geometry, or silhouette.

**Architecture:** Extend catalog records with typed optional material channels, resolve them from stable `PartId`s, and render them through one Bevy 2D material while retaining the existing sprite path as a deterministic fallback. The same cutout hierarchy remains authoritative in creation, shop, and arena.

**Tech stack:** Rust, Bevy ECS/`Material2d`, WGSL, PNG channel assets, and the repository's `xtask` asset/browser harness.

## Global Constraints

**Guardrails:** Keep `CharacterDefinition`/`PartId` persistence unchanged; all material paths live only in the versioned catalog. Keep the right-facing cutout art, existing `CutoutPartKind` attachment points, transforms, pivots, and `flip_x` behaviour. The first wardrobe is the existing Romanian `ie`/`ițari`/opinci human, not generic fantasy art. Optional channels must never make a selected part disappear: use the existing albedo `Sprite` path deterministically.

## Task 1 — Version and validate the material authoring contract

**Files:** modify `src/character/catalog.rs`, `src/character/mod.rs`, `assets/fighters/catalog/human-foundation.json`, `xtask/src/assets/validate/refs.rs` (and its tests); add records to `assets/fighters/human/runtime/manifest.toml`.

1. Write failing catalog/asset-validation tests for a `PartRecord.material` contract: required albedo stays `asset_path`; optional `mask_path`, `normal_path`, `shadow_path`, `palette`, `depth_offset`, and `highlight` are validated when supplied. Reject an unregistered channel, a channel whose runtime attachment differs from the part, and a non-finite/out-of-range numeric setting.
2. Introduce typed `MaterialMetadata`/`PaletteRegion` (no raw renderer configuration in JSON) and bump `CHARACTER_CATALOG_VERSION` to 2. Migrate the bundled catalog to v2. Do **not** bump `CHARACTER_DEFINITION_VERSION`: selected stable IDs and saves have identical meaning.
3. Extend the catalog-aware asset reference check so every material channel is registered in the human runtime sidecar and has the same attachment as the owning `PartId`; update its fixture tests.

Run: `cargo test --lib character::catalog` and `cargo test -p xtask assets::validate` and `cargo xtask assets check`.

## Task 2 — Resolve and render the hybrid material with a safe sprite fallback

**Files:** add `src/character/material.rs` and `assets/shaders/hybrid_character_2d.wgsl`; modify `src/character/mod.rs`, `src/cutout.rs`, `src/lib.rs` only if plugin registration requires it.

1. Write headless tests first for `resolve_material_for_part(&PartRecord) -> ResolvedPartMaterial`: it is keyed by the resolved record/`PartId`, returns palette/depth/shadow settings, and returns `None` for absent optional channels.
2. Add a small Bevy `Material2d`/WGSL material that samples albedo, mask, normal, and shadow, performs palette replacement plus restrained fixed-direction lighting, and preserves alpha/pixel edges. Keep lighting and contact-shadow values bounded by the typed metadata.
3. Extend `CutoutPart` with resolved material data (not identity/path ownership) and make `rig_template_for` transfer it alongside the existing `source_id` and albedo. In `part_sprite`/spawn, use the hybrid material only when every required runtime handle is available; otherwise spawn the current albedo `Sprite` with the same size, tint, transform, z-order, source ID, hierarchy, and mirror.

Run: `cargo test --lib character::material cutout::`; add assertions that normal and mirrored complete-human rigs retain exactly the same `PartId`s, rest transforms, part count, and fallback sprite silhouette.

## Task 3 — Supply one complete Romanian human material set and document its contract

**Files:** add representative channel PNGs under `assets/fighters/human/runtime/`; modify `assets/fighters/human/runtime/manifest.toml`, `assets/fighters/catalog/human-foundation.json`, `assets/CREDITS.md`, and `docs/art-direction.md`.

1. Produce/register channels for every visible selection of `known_good_human` (body chains, face, braided hair, linen `ie`, ițari, and opinci): albedo remains the existing cutout; masks expose only documented embroidery/cloth/hair regions; normals/shadows are restrained and share the relevant attachment/pivot.
2. Put the representative palette and depth profile on those existing stable records. Preserve their current silhouette/crop and Romanian motif; record cultural reference plus rights/provenance in the manifest and credits.
3. Add an authoring section to `docs/art-direction.md`: right-facing transparent layers, nearest/pixel-consistent finish, one light direction, mask colours/meaning, normal/shadow restraint, and the fact that missing channels intentionally fall back to albedo.

Run: `cargo xtask assets check && cargo xtask assets gallery` (inspect both facings/poses), then `cargo test --lib character::catalog cutout::`.

## Task 4 — Prove the shared end-to-end slice in browser CI

**Files:** add `xtask/src/web_smoke/hybrid_2_5d_character.rs`; modify `xtask/src/web_smoke/mod.rs`, `xtask/src/commands/web_smoke_cmd.rs`, `src/review/mod.rs`; add accepted baselines under `tests/visual/baselines/hybrid-2-5d-character/` only after rebasing on `origin/main`.

1. Add review-only snapshot data that exposes the six selected stable IDs, whether the hybrid material or deterministic albedo fallback was used, and the preview/combat roots' part counts.
2. Create one scenario that drives creation → shop → fight for the seeded representative human at desktop and phone viewports. Assert the exact same six IDs and part count in all three views, then run once with material inputs disabled to assert the albedo fallback is visible and identity/silhouette facts do not change.
3. Capture/review the two viewport baselines only after the branch is rebased; retain existing `gold-journey` as the broader screen matrix.

Run: `cargo xtask web-smoke --scenario hybrid-2-5d-character --strict-visual`, `cargo xtask web-smoke --scenario gold-journey`, then `cargo xtask pre-push` before push.
