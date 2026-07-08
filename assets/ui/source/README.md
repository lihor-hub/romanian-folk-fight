# UI Presentation Motifs Source Sheet

`ui_presentation_motifs_v1.png` is a production-intent source sheet for the
pixel-art menu and HUD reskin direction.

It is not wired into runtime rendering yet. Later UI work should slice this
sheet into individual panel, icon-frame, bar-cap, and divider assets or replace
it with cleaned artist-authored pieces that preserve the same style.

## Intended Parts

- Embroidered banner strip
- Ornate corner pieces
- Coin medallion
- Gear slot frames
- Health and stamina bar end caps
- Divider knots
- Menu button end caps

## Style Contract

- Polished pixel-art UI pieces with crisp dark outlines.
- Romanian textile motifs built from diamonds, crosses, and zig-zag borders.
- Deep red, cream, black, and gold palette matching `docs/art-direction.md`.
- No text, letters, or numbers baked into the source sheet.
- Transparent background after chroma-key cleanup.

## Runtime Status

This is a source asset for issue #101. It does not replace `assets/ui/*.png` and
does not change the Bevy runtime path yet.
