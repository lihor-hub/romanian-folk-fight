# Next Pixel-Art Asset Batch Brief

**Goal:** Prepare the next independent pixel-art cutout source sheets for
generation after the first human/player body slice.

**Sources:** `docs/art-direction.md` and
`docs/superpowers/specs/2026-07-08-pixel-art-cutout-assets-design.md`.

**Batch constraints:**

- Generate raster pixel-art source sheets only; do not add procedural asset
  scripts.
- Keep every sheet modular: isolated transparent parts, authored facing right,
  with generous padding and no full-frame animation strips.
- Match the shared palette: `#7a1f1f`, `#1a1214`, `#e8dcc8`, `#c9a227`, plus at
  most one muted accent hue per enemy template.
- Preserve the production contract: consistent outline weight, readable
  silhouettes at 192-256px tall, Romanian textile motifs, and documented rights
  for every accepted file.

## Recommended Order

1. **Starter gear sheet** - Makes the human cutout useful for loadout/shop
   screenshots without requiring new rig proportions.
2. **Non-human enemy template** - Proves the cutout style works beyond recolored
   humans while staying close to normal fighter scale.
3. **Zmeu boss template** - Tests oversized proportions after the smaller
   humanoid and enemy sheets establish style consistency.
4. **UI presentation motifs** - Gives the menu/HUD reskin a matching source-art
   vocabulary without wiring runtime assets in this slice.

## Sheet 1: Starter Gear Cutout Parts

**Prompt brief:** A polished pixel-art cutout source sheet of Romanian folklore
arena gear parts, isolated on transparent background, authored for a right-facing
side-view modular fighter. Include `bâtă ciobănească`, topor, paloș, wooden
shield, ferecat shield, ie, cojoc, chain shirt, căciulă, coif, opinci, and boots.
Use thick dark outlines, crisp clusters, cream linen, deep red and gold textile
accents, black leather/iron details, and no labels or shadows.

**Acceptance checks:**

- Each item is separated with enough padding to slice into an attachable part.
- Weapons sit at a natural hand angle; shields read as forearm-mounted; hats,
  torso gear, and footwear align with a right-facing human rig.
- No item is drawn as a centered full-body overlay.
- Motifs are geometric and readable, not noisy fabric texture.

## Sheet 2: Strigoi Enemy Cutout Template

**Prompt brief:** A polished pixel-art cutout source sheet for one strigoi enemy
template, isolated on transparent background, authored facing right. Include
separate head, gaunt torso, upper arms, forearms, clawed hands, thighs, shins,
feet, ragged hair, face features, and torn folk-clothing scraps. Keep the shared
palette, add one muted pale ash accent hue, and give the silhouette distinct
thin, undead proportions without breaking the 192-256px arena readability.

**Acceptance checks:**

- The template is visibly non-human through proportions and face/features, not
  only recolor.
- Parts map to the same cutout categories as the human rig where practical.
- Claws, hair, and torn clothing are separate enough for later layering.
- Light direction, outline weight, and pixel scale match the human/gearing
  direction.

## Sheet 3: Zmeu Boss Cutout Template

**Prompt brief:** A polished pixel-art cutout source sheet for one large zmeu boss
template, isolated on transparent background, authored facing right. Include
separate oversized head, broad torso, upper arms, forearms, large hands, thighs,
shins, heavy feet, hair or horn-like headgear, boss belt/trim, and optional torso
armor. Keep the shared palette, add one muted storm-gray accent hue, and make the
boss read as larger and heavier than the player while still using modular cutout
parts.

**Acceptance checks:**

- The silhouette clearly reads as boss-scale through torso, head, and arm mass.
- Parts remain isolated and attachable, not painted into a single full-body
  sprite.
- Romanian folk trim appears on belt, armor, or clothing bands without becoming
  visual noise.
- The sheet stays compatible with idle, attack, block, dodge, hurt, and KO pose
  definitions through runtime transforms.

## Sheet 4: UI Presentation Motifs

**Prompt brief:** A polished pixel-art UI source sheet of Romanian textile
presentation pieces, isolated on transparent background. Include an embroidered
banner strip, ornate corners, slot frames, health and stamina bar end caps,
divider knots, a coin medallion, and menu button end caps. Use geometric
diamonds, crosses, and zig-zags in the shared deep red, black, cream, and gold
palette. Do not include readable letters, numbers, labels, or full screenshots.

**Acceptance checks:**

- Each UI element is separated with enough padding to slice into individual
  panel, frame, bar, or divider assets.
- Motifs match the fighter gear textile language without copying any existing
  game UI.
- No text is baked into the image.
- The sheet is source-only and does not replace `assets/ui/*.png` in this PR.
