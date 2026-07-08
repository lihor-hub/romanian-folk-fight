# Zmeu Cutout Source Sheet

`zmeu_cutout_parts_v1.png` is a production-intent source sheet for the first
large boss-scale pixel-art cutout template.

It is not wired into runtime rendering yet. Later skeleton-template work should
either slice this sheet into individual transparent body parts or replace it
with cleaned artist-authored parts that preserve the same boss silhouette.

## Intended Parts

- Oversized heads
- Broad torso
- Upper arms
- Forearms
- Large hands
- Thighs
- Shins
- Heavy feet
- Belt and cloth overlays
- Shoulder or torso armor pieces
- Hair, horn-like headgear, and boss trim

## Style Contract

- Side-view boss parts authored facing right.
- Boss-scale proportions: broad chest, heavy limbs, and a larger silhouette than
  the human and strigoi templates.
- Same outline weight, pixel scale, and upper-left light direction as the other
  pixel-art cutout source sheets.
- Uses the shared art-direction palette plus a muted storm-gray accent.
- Transparent background after chroma-key cleanup.

## Runtime Status

This is a source asset for issue #101. It does not replace
`assets/sprites/zmeu.png` or `assets/sprites/zmeul_zmeilor.png` and does not
change the Bevy runtime path yet.
