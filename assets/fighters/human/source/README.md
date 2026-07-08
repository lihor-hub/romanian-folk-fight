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
