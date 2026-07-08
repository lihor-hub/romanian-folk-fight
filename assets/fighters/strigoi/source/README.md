# Strigoi Cutout Source Sheet

`strigoi_cutout_parts_v1.png` is a production-intent source sheet for the first
non-human pixel-art cutout enemy template.

It is not wired into runtime rendering yet. Later skeleton-template work should
either slice this sheet into individual transparent body parts or replace it
with cleaned artist-authored parts that preserve the same silhouette language.

## Intended Parts

- Gaunt torso
- Pale angular head and hair
- Upper arms
- Forearms
- Clawed hands
- Thighs
- Shins
- Feet
- Ragged cloth overlays
- Belt and sash details

## Style Contract

- Side-view enemy parts authored facing right.
- Non-human proportions: leaner, longer arms, sharper claws, and a more hunched
  silhouette than the human base.
- Same outline weight, pixel scale, and upper-left light direction as the human
  and gear source sheets.
- Uses the shared art-direction palette plus a muted ash-gray/blue accent.
- Transparent background after chroma-key cleanup.

## Runtime Status

This is a source asset for issue #101. It does not replace
`assets/sprites/strigoi.png` and does not change the Bevy runtime path yet.
