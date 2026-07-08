# Art direction — Romanian Folk Fight

Reference doc for all Phase 4 ("remastered presentation") issues.

## Look

- Polished pixel-art cutout fighters in a theatrical side-view arena.
- The presentation should keep the Swords-and-Sandals-like paper-doll feel:
  modular bodies, visible gear, expressive faces, and readable combat poses.
- The style is project-owned Romanian folklore pixel art, not copied Flash art
  and not low-detail placeholder pixel art.
- Chunky, readable silhouettes; characters must read at ~192-256px tall.
- Fighters are authored facing **right**; the engine mirrors the opponent
  with `flip_x`.
- Use transparent PNG parts with crisp dark outlines, clean clusters, and only
  as much dithering as helps material reads. Avoid noisy texture.

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
  panel borders, clothing bands, shields, belts, and the announcer banner.
- Keep motifs geometric and repeatable; no photorealistic texture.

## Production asset model

- Production fighters should be built from transparent PNG body parts, not
  full-frame sprite sheets. The runtime rig moves, rotates, and layers those
  parts for idle, attack, block, dodge, hurt, and KO poses.
- Body parts should include a documented pivot/attachment point for the rig:
  head, torso, upper arms, forearms, hands, thighs, shins, feet, hair/facial
  hair, face features, weapons, shields, headgear, torso gear, and footwear.
- Gear must be authored as attachable parts, not centered full-frame overlays.
  Weapons sit in hands, shields on arms, hats/helmets on heads, armor over
  torsos, and boots/opinci on feet.
- The existing `assets/sprites/*.png`, `assets/gear/*.png`, and Python
  generator scripts are bootstrap placeholders. Do not use those scripts as the
  production asset pipeline.
- Production assets may be AI-generated, artist-authored, or cleaned up by hand,
  but every accepted file must have project-owned rights and an entry in
  `assets/CREDITS.md`.

## First playable asset table

This is the first production-intent batch to generate or author before replacing
the placeholders in play.

| Group | Minimum assets | Purpose |
| --- | --- | --- |
| Human base rig | Torso, head, upper/lower arms, hands, upper/lower legs, feet | Shared body for the player and human-like enemies. |
| Hero identity | Four faces, four hair/beard sets, four clothing accent sets, four skin tones | Supports Haiducul, Voinicul, Ciobanul, and Ucenicul Solomonar presets plus custom creation. |
| Starter gear | Bâtă ciobănească, topor, paloș, wooden shield, ferecat shield, ie, cojoc, chain shirt, căciulă, coif, opinci, boots | Covers the gear slots already present in the shop and loadout systems. |
| Non-human enemy | One strigoi or vârcolac body template with distinct proportions | Proves the rig can support more than recolored humans. |
| Large boss | One zmeu-style large body template with oversized torso/head/arms | Proves boss-scale silhouettes and attachment points. |
| Arena compatibility | Idle, attack, block, dodge, hurt, KO pose definitions for each template | Keeps combat readable without redrawing full animation frames. |

## Generation guidance

- Generate final candidates as raster pixel-art PNGs or layered source files,
  not by writing new procedural Python image scripts.
- Ask for isolated transparent parts on a neutral canvas, facing right, with the
  same palette and outline weight across the set.
- Prefer a small number of excellent reusable parts over many inconsistent
  variants. A first slice should make one complete player, one enemy, and one
  boss look intentional end to end.
- After generation, curate hard: remove mismatched outlines, inconsistent light
  direction, unreadable motifs, and any asset whose rights cannot be documented.
