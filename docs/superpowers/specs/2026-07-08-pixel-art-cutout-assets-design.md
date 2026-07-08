# Pixel-Art Cutout Assets Design

## Decision

Romanian Folk Fight should pivot Phase 4 art from Flash/vector cutout fighters
to polished pixel-art cutout fighters. The rig concept remains: fighters are
assembled from transparent body and gear parts, then animated through runtime
transforms. The visual finish changes to crisp, high-detail pixel art with thick
dark outlines and Romanian folk motifs.

This keeps the useful Swords-and-Sandals-like structure: player identity,
paper-doll gear, readable poses, and customizable parts. It avoids copying that
game's Flash look and avoids multiplying the workload with full-frame sprite
sheets for every animation and equipment combination.

## Reader

This spec is for agents or artists working on issues #88, #89, #90, #92, #93,
and #95. It should answer what to generate, what not to generate, and how the
asset batch fits the Bevy rig work.

## Production Target

- Fighters are side-view, authored facing right, and mirrored by the engine for
  opponents.
- Fighters should read at roughly 192-256px tall in-game.
- Assets are transparent PNG parts with stable pivots and attachment points.
- The runtime rig handles motion. Production art should not be full-frame
  animation sheets unless a future issue explicitly changes this.
- Gear is attachable to body parts, not centered over a whole fighter frame.
- Existing Python-generated files are bootstrap placeholders only.

## Asset Table

| Group | Minimum assets | Purpose |
| --- | --- | --- |
| Human base rig | Torso, head, upper/lower arms, hands, upper/lower legs, feet | Shared body for player and human-like enemies. |
| Hero identity | Four faces, four hair/beard sets, four clothing accent sets, four skin tones | Supports Haiducul, Voinicul, Ciobanul, Ucenicul Solomonar, and custom creation. |
| Starter gear | Bâtă ciobănească, topor, paloș, wooden shield, ferecat shield, ie, cojoc, chain shirt, căciulă, coif, opinci, boots | Covers current shop/loadout slots. |
| Non-human enemy | One strigoi or vârcolac template with distinct proportions | Proves template support beyond recolored humans. |
| Large boss | One zmeu-style large template with oversized proportions | Proves boss-scale rendering. |
| Pose definitions | Idle, attack, block, dodge, hurt, KO for each template | Makes combat readable through transforms. |

## Generation Approach

Use image generation or human-authored drawing for first candidates. Do not add
new procedural Python scripts for production assets. The generation prompt should
ask for clean, isolated pixel-art body parts, consistent outlines, Romanian folk
textile accents, and transparent backgrounds.

Generated assets still need curation. Accept only files with consistent outline
weight, light direction, pixel scale, palette, and documented rights. Update
`assets/CREDITS.md` for every accepted file.

## Scope Boundaries

In scope:

- Documentation and roadmap alignment for the pixel-art cutout direction.
- A first asset batch table for generation or artist work.
- Preserving the modular rig architecture already planned by the Phase 4 issues.

Out of scope:

- Music and sound replacement. The user plans to bring music later.
- Replacing all existing placeholder files in this documentation slice.
- Adding new procedural asset-generation scripts.

## Self-Review

- Placeholder assets are described only as legacy/bootstrap material, not as a
  production requirement.
- The spec does not require copying Swords and Sandals art or assets.
- The spec keeps the runtime rig direction compatible with gear attachment and
  jointed combat pose issues.
- The asset table is small enough for a first playable visual slice.
