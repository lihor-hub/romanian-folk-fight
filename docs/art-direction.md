# Art direction — Romanian Folk Fight

Reference doc for all Phase 4 ("remastered presentation") issues.

## Look

- Hand-drawn / painted 2D, side-view fighters in a theatrical arena.
- Chunky, readable silhouettes; characters must read at ~128px tall.
- Fighters are authored facing **right**; the engine mirrors the opponent
  with `flip_x`.

## Palette

Drawn from Romanian folk textiles (traditional ii and woven belts):

| Role | Color | Hex |
| --- | --- | --- |
| Deep red | primary accent, blood-red wool | `#7a1f1f` |
| Black | outlines, hair, boots, night sky | `#1a1214` |
| Cream | linen shirts, highlights, UI text | `#e8dcc8` |
| Gold | trim, embroidery, boss accents | `#c9a227` |

Each creature adds **one** muted accent hue of its own (forest green, pale
ash, storm gray, ...) on top of this shared base so the roster stays
coherent but every opponent is distinct at a glance.

## Motifs

- Embroidered ii patterns (diamonds, crosses, zig-zag borders) for UI trim,
  panel borders, and the announcer banner.
- Keep motifs geometric and repeatable; no photorealistic texture.

## Sprite sheets

- One sheet per fighter under `assets/sprites/`, 4x3 grid of 128x128 frames:
  idle (4), attack (4), hurt (2), KO (2) — the layout consumed by
  `src/arena/animation.rs`.
- Current sheets are self-generated pixel-art placeholders
  (`scripts/generate-placeholder-sprites.py`); bespoke painted art per
  folklore creature is follow-up work and must keep the same frame layout.
