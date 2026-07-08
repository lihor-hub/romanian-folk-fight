# Starter Gear Cutout Source Sheet

`starter_gear_cutout_parts_v1.png` is a production-intent source sheet for the
first pixel-art gear set.

It is not wired into runtime rendering yet. Later rig work should slice this
sheet into individual transparent attachment parts or replace it with cleaned
artist-authored part files that preserve the same style.

## Intended Parts

- Bata ciobaneasca
- Topor
- Palos
- Wooden shield
- Iron-banded ferecat shield
- Ie torso gear
- Cojoc torso gear
- Chain shirt torso gear
- Caciula
- Coif
- Opinci
- Boots

## Style Contract

- Gear is authored as attachable cutout parts, not centered full-frame overlays.
- Weapons are sized for a right-facing 192-256px tall fighter.
- Shields, torso gear, headgear, and footwear should be sliced as independent
  attachments in later runtime work.
- Thick dark outlines, clean pixel clusters, and Romanian folk textile motifs
  match `docs/art-direction.md`.
- Transparent background after chroma-key cleanup.

## Runtime Status

This is a source asset for issue #101. It does not replace the placeholder
`assets/gear/*.png` overlays and does not change the Bevy runtime path yet.
