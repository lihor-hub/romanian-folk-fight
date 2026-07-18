# Romanian paper-doll v1 production source

This directory is the auditable source record for the Haiduc and Cioban
production-intent paper-doll looks and the five replacement equipment assets.
The tracked PNG is a 128-colour, nearest-scaled contact sheet of both original
chroma masters. At 340 KB it stays below the repository's 1 MB hook while
remaining useful for visual comparison. Runtime crops are reproduced from the
full-resolution originals by `scripts/extract-romanian-paper-doll-v1.py`; the
contact sheet is not silently substituted for those originals.

These designs are a pan-Romanian folkloric remix for a fantasy fighting game,
not a claim that one exact historical or regional costume combined every shown
piece. Research anchors were the Romanian National Heritage Institute's
[altiță record](https://www.patrimoniu.ro/en//articles/arta-camasii-cu-altita-element-de-identitate-culturala-in-romania),
the INP/cIMeC [Țara Oltului port-popular collection record](https://cartipostale.cimec.ro/Detaliu.php?id=933),
and its [Maramureș port record](https://cartipostale.cimec.ro/Detaliu.php?criteriu=port+popular&id=6604).
They guided the shoulder embroidery, cream shirts, white ițari, dark wool
cioareci, wrapped leather opinci, practical sheepskin cojoc, and dark wool
căciulă grammar. No museum image or earlier game art was copied into the
generated pixels.

## Source and rights

- Mode: OpenAI built-in image generation, followed by the installed imagegen
  chroma-key helper and deterministic local curation.
- Rights: both masters were generated for this project. The accepted albedos,
  their deterministic technical maps, and the tracked contact sheet use the
  repository asset wording, “Same as project assets unless superseded.”
- Human master:
  `/Users/ioachimlihor/.codex/generated_images/019f701f-492d-70e1-9c8a-e64740c1d407/exec-0249e3ae-514e-4c04-aef9-9721358f2ddf.png`
  (`8518d470aae529898a79ed22303cb97b4d358b13fa7bd80088dfac2c0d7334d7`).
- Equipment master:
  `/Users/ioachimlihor/.codex/generated_images/019f701f-492d-70e1-9c8a-e64740c1d407/exec-51bc0f86-e059-429c-89ba-56447e74e048.png`
  (`84b35457846ed8b7e8707e2aedf135cb2a94b3c4b900648307c7215b26f8e0a9`).

The extraction script verifies both SHA-256 values before writing any output.
It runs the installed `remove_chroma_key.py` helper with border auto-key,
soft matte, thresholds 12/220, and despill. `--check` then proves the 34
accepted albedos, 102 exact-alpha technical maps, three generated manifests,
and tracked source contact sheet are byte-identical.

## Exact human prompt

```text
Use case: stylized-concept
Asset type: production source master sheet for a modular 2.5D pixel-art paper-doll game character
Primary request: create a single clean source sheet containing two culturally Romanian right-facing articulated human doll sets, Haiduc and Cioban, with every reusable part visibly separated and non-overlapping for later cropping
Reference image: use the previous Romanian haiduc material reference only for palette, embroidery restraint, pixel density, and upper-left lighting; do not copy its composed layout
Parts required for EACH look: head/face, hair, torso garment, upper arm back, forearm back, hand back, upper arm front, forearm front, hand front, thigh back, shin back, thigh front, shin front. Also include shared foot back, foot front, and a separate short-hair variant.
Haiduc design: lean silhouette; long loose dark hair; white linen ie with restrained red-and-black altiță/geometric embroidery; white ițari.
Cioban design: sturdier silhouette; tied dark hair; cream shepherd shirt with restrained black/brown geometric edging; dark wool cioareci.
Shared feet: traditional brown leather opinci with wrapped ties.
Style/medium: crisp high-resolution pixel art, subtle 2.5D volume, modular Swords-and-Sandals-like paper-doll readability, consistent charcoal outline weight, restrained highlights
Composition/framing: orthographic asset sheet grid, all parts isolated with generous uniform spacing, each part fully visible, same scale and right-facing orientation, no overlaps, no labels or text
Lighting/mood: fixed soft upper-left light; shallow volume only
Scene/backdrop: perfectly flat solid #00ff00 chroma-key background, uniform edge to edge, no floor, gradients, texture, shadows, or lighting variation
Constraints: Romanian folk wardrobe, non-caricatured anatomy, identical pixel density across all parts, compatible joint ends and coherent limb thickness within each body, crisp hard alpha-ready edges; no generic fantasy armour, no weapons, no full composed character, no cast shadows, no watermark, no text; do not use #00ff00 in any part
```

## Exact equipment prompt

```text
Use case: stylized-concept
Asset type: production source sheet for modular Romanian folk-fight paper-doll equipment
Primary request: create exactly five separate right-facing equipment assets, isolated and non-overlapping: (1) traditional shepherd's staff / bâtă ciobănească, (2) compact woodsman's axe / topor de pădurar, (3) cream-brown sheepskin cojoc vest shown as a torso overlay, (4) dark brown wool căciulă de oaie shown as a head overlay, (5) pair presentation of brown leather opinci with wrapped ties as a feet overlay
Style/medium: crisp high-resolution pixel art matching a subtle 2.5D Romanian paper-doll fighter, consistent charcoal outlines, restrained upper-left highlights, readable at small game scale
Composition/framing: orthographic asset grid, each item completely isolated with generous spacing, no labels, consistent pixel density, attachment-friendly orientation
Scene/backdrop: perfectly flat solid #00ff00 chroma-key background edge to edge; no floor, gradient, texture, reflection, cast shadow, or lighting variation
Constraints: culturally Romanian folk equipment, practical handmade wood/wool/leather construction; no generic fantasy ornament, no modern objects, no characters, no extra items, no text, no watermark; do not use #00ff00 inside assets
```

## Extraction and rig contract

`PARTS` in the extraction script is the exact accepted map. It records each
full-resolution crop rectangle, output size, mask semantics, and any curated
face/hair treatment. The source model drew some requested arms and legs as
whole assemblies. The script therefore splits only two coherent arm chains
per look at overlapping embroidered-cuff seams and only two coherent leg
chains at overlapping knee seams. Third sleeve/hand candidates, extra opinci,
the frontal shoe, and repeated feet are deliberately rejected.

All runtime art faces right. The engine alone mirrors it. Albedos use the
existing attachment pivots and display sizes at a consistent two source pixels
per displayed pixel; nearest sampling preserves the charcoal outline and motif
clusters. Lighting is shallow and fixed from the upper left.

Every accepted albedo has `_mask.png`, `_normal.png`, and `_shadow.png`
companions with byte-identical alpha. Mask RGB follows catalog palette order:
red is skin, hair, cloth, or leather as declared by the layer; green is the
second declared region, normally embroidery or cloth; blue is leather only on
three-region shin maps. Black RGB means no recolour. Normals stay near flat
blue, and shadows remain soft white-to-light-gray local depth signals.
