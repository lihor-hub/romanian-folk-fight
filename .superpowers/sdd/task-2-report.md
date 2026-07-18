# Task 2 report — hybrid Material2d renderer with sprite fallback

## Outcome

Implemented the Bevy 0.19 hybrid character material tracer bullet while keeping
the existing cutout `Sprite` as the deterministic initial and fallback render
path. No representative channel PNGs or catalog material entries were added;
those remain Task 3.

## Implementation

- Added `character::material` with:
  - `resolve_material_for_part(&PartRecord) -> ResolvedPartMaterial`, keyed by
    the catalog `PartId` and preserving absent optional channels as `None`;
  - defensive bounds for depth, highlight, and renderer-owned contact shadow;
  - `HybridCharacterMaterial`, a Bevy `Material2d` with four image channels;
  - a pending ECS component and promotion system that waits until albedo, mask,
    normal, and shadow handles all exist in `Assets<Image>`.
- Added `hybrid_character_2d.wgsl` with integer texel sampling, palette-mask
  replacement, fixed-direction restrained normal lighting, bounded contact
  shadow, mirrored UV/normal handling, and a 0.5 alpha cutout.
- Extended `CutoutPart` with resolved material data. `rig_template_for` copies
  it only where the selected record's attachment supplies that part's existing
  albedo, leaving broader shared `source_id` mapping unchanged.
- The spawn path always creates the current sprite first. A complete authored
  set gets a pending component; incomplete, missing, loading, or failed channel
  sets remain the albedo sprite. Promotion removes only `Sprite` and adds a
  same-size rectangle mesh/material, retaining entity, `PartId`, transform,
  rest pose, z-order, hierarchy, tint, and facing.
- `CutoutRigPlugin` registers the material plugin when the real asset runtime is
  present, while lightweight headless apps remain supported.

## TDD evidence

Red failures observed before implementation:

- resolver tests panicked at the explicit Task 2 `todo!`;
- alpha-mode test reported `Opaque` instead of `Mask(0.5)`;
- async promotion test retained `Sprite` after all four test images arrived;
- rig-adapter test reported no transferred torso material.

Each failure was followed by the minimum implementation and a focused green
run. Added coverage for absent channels, defensive numeric bounds, all-handle
promotion gating, unchanged transforms, async spawn fallback, stable identity
and geometry, normal/mirrored part count, and exact fallback custom size.

## Verification

- `cargo fmt --all -- --check` — pass.
- `cargo clippy --all-targets -- -D warnings` — pass.
- `cargo test --lib character::material` — 5 passed.
- `cargo test --lib cutout::` — 19 passed.
- `cargo test --lib tests::game_plugin_builds_without_duplicate_plugins` — 1 passed.
- `git diff --check` — pass.

The implementation-plan command containing two Cargo test filters is not valid
Cargo CLI syntax, so the two requested focused filters were run separately.

## Self-review

- Standards: ECS behavior is a focused system under the existing rig plugin;
  runtime code has no new unwrap/expect paths; no dependency or feature changes.
- Spec: promotion is deterministic and asynchronous; missing channels never
  remove the fallback; mesh sizing and shader mirroring avoid transform or
  silhouette mutation; Task 3 assets remain out of scope.
- Shader review: all authored numeric effects are clamped in both Rust and
  WGSL; zero-length malformed normals fall back to a neutral forward normal;
  mask, normal, and shadow PNGs explicitly load in linear rather than sRGB
  space.

## Concerns

The focused headless suite proves material construction and ECS promotion, but
does not create a GPU render pipeline. Visual/browser validation awaits Task 3,
when complete registered PNG channel sets exist.
