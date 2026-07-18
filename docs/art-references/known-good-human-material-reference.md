# Known-good human material reference

This record preserves the cultural, visual, and rights provenance for the
first Romanian human hybrid-material channel set. The reference image guided
palette, material boundaries, and the restrained upper-left light direction
only. It is not runtime art, is never sampled by the game, and was not used to
redraw or replace any accepted albedo silhouette.

## Source and rights

- Source: OpenAI built-in image generation, generated for this project.
- Original output path at generation time:
  `/Users/ioachimlihor/.codex/generated_images/019f701f-492d-70e1-9c8a-e64740c1d407/exec-08a4ca26-dc9a-405e-9b7d-8b22895c0a84.png`
- Original SHA-256:
  `2b57e1553c0b83c529ae593987f4ed44988a4d020a7c9f30a7536bba7919de37`
- Rights: generated for the project; the checked-in technical maps are
  deterministic derivatives of the already credited project albedos and use
  the repository asset license wording, “Same as project assets unless
  superseded.”

An exact internal copy of the 1.867 MB reference was attempted. The required
`check-added-large-files` pre-commit hook rejected it because the repository
limit is 1 MB. The hook was not weakened, and no lossy derivative was created
merely to bypass the limit. The binary is therefore intentionally excluded;
this tracked record retains the exact prompt, source location, hash, and
decision needed to audit its use.

## Exact prompt

```text
Use case: stylized-concept
Asset type: internal game-art reference sheet for a Romanian folk fighting game
Primary request: create a concise visual reference sheet for a hybrid 2.5D pixel-art paper-doll fighter material treatment
Subject: one right-facing Romanian haiduc fighter wearing a white linen ie with restrained red-and-black geometric embroidery, ițari trousers, opinci, braided dark hair, shown as a clean cutout plus three small material callouts: recolor mask regions, restrained tangent-like normal shading, and soft contact-shadow/depth treatment
Style/medium: crisp high-resolution pixel art with subtle 2.5D volume, articulated paper-doll/cutout character, Swords-and-Sandals-like modular readability but culturally specific Romanian wardrobe
Composition/framing: neutral reference-board layout, full body fully visible, clear silhouette, small material callout swatches beside it
Lighting/mood: fixed soft light from upper left, restrained highlights, readable flat planes, no realistic 3D rendering
Color palette: warm linen, charcoal outlines, muted leather brown, restrained Romanian red/black embroidery
Constraints: culturally coherent Romanian folk wardrobe; no generic medieval plate armor; no text, logos, watermark, scenery, weapons, cast shadow, or cropped limbs; preserve pixel-crisp edges and modular layer readability
```

## Checked-in derivative contract

`scripts/generate-human-material-channels.py` derives mask, normal, and shadow
maps only from the accepted runtime albedos. All companion maps retain the
albedo dimensions and byte-identical alpha silhouette. Masks use RGB for at
most three positional semantic regions; alpha remains exclusively the
silhouette. Normals stay near tangent-space flat blue, and shadows remain a
restrained grayscale signal.
