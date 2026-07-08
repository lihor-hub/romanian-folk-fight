# Human Cutout Source Sheet

`human_cutout_parts_v1.png` is the first production-intent source sheet for the
pixel-art cutout fighter direction.

It is not wired into runtime rendering yet. Later rig work should either slice
this sheet into individual transparent parts or replace it with cleaned
individual part files that preserve the same style.

## Intended Parts

- Head
- Hair and moustache
- Torso with cream linen shirt and red/gold folk accents
- Upper arms
- Forearms
- Hands
- Upper legs
- Shins
- Feet/opinci

## Style Contract

- Side-view fighter parts authored facing right.
- Thick dark outlines using the shared art-direction palette.
- Polished pixel-art clusters, not painterly texture.
- Romanian folk textile accents on clothing bands and belt areas.
- Transparent background after chroma-key cleanup.

## Runtime Status

This is a source asset for issue #99. It does not replace `assets/sprites/player.png`
and does not change the Bevy runtime path yet.

## Preset-first variants

`scripts/generate-hero-preset-parts.py` produces the preset-first PNG
variants (`torso_*`, `head_moustache.png`, `head_beard.png`,
`hair_alternate.png`, `hair_ornate.png`) that the cutout rig picks per
`CostumeStyle`, `HeadFeature`, and `HairVariant`. Regenerate them by running
the script.

### Silhouette-variant PNG convention

- Every preset appearance value with a variant PNG must resolve to a PNG
  whose opaque-pixel silhouette is visibly distinct from the neutral base —
  preset identity has to read on the sprite layer even before colour is
  applied. Palette-only swaps are not enough.
- `CostumeStyle` slot: each of the four predefined heroes owns its own
  `torso_<preset>.png` (`torso_haiduc_coat.png`, `torso_voinic_tunic.png`,
  `torso_cioban_cojoc.png`, `torso_solomonar_robe.png`). The neutral
  `torso.png` is reserved for the non-preset base rig used by generic human
  enemies and must never be selected by a `CostumeStyle`.
- `HeadFeature` slot: `Clean` uses `head.png`; `Moustache` and `Beard`
  layer distinct facial-hair silhouettes into the head via
  `head_moustache.png` / `head_beard.png`.
- `HairVariant` slot: `Primary` uses `hair.png`; `Alternate` and `Ornate`
  supply short-cropped and long-mane silhouettes via
  `hair_alternate.png` / `hair_ornate.png` so presets that share a
  `HairStyle` still differ visually.
- When adding a new preset costume, head-feature, or hair-variant value,
  add a matching PNG to this directory, wire it through
  `src/cutout.rs::costume_torso_asset_path` (or the head-feature /
  hair-variant sibling), and add a corresponding row to
  `assets/CREDITS.md`. The distinctness invariants are enforced by
  `all_four_hero_presets_produce_distinct_costume_torso_asset_paths` and
  `preset_head_feature_and_hair_variant_paths_reflect_taxonomy` in
  `src/cutout.rs`.
