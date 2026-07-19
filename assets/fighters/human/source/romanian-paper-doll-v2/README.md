# Romanian paper-doll v2 source record

This directory is the auditable source record for the Voinic and Ucenic
Solomonar production presets added by issue #326. The two tracked chroma
masters were created with OpenAI's built-in image-generation tool on 2026-07-19.
They are source contact sheets only; the game never performs runtime image
generation.

The final built-in prompts requested isolated right-facing attachment parts on
a uniform green field, in the established crisp pixel-adjacent 2.5D treatment.
The Voinic prompt specified a cream linen cămașă voinicească, restrained
red/black geometric embroidery, broad leather brâu, dark navy wool cioareci,
short swept hair, and no fantasy armour. The Ucenic Solomonar prompt specified
a charcoal short wool suman over cream linen, restrained white/dark-red trim,
oxblood brâu and leg ties, pale wool cioareci, a youthful clean-shaven face,
raven forelock, and explicitly excluded wizard hats, runes, generic robes,
armour, and weapons. Both prompts used the v1 contact sheet as a style and
spacing reference only.

## Exact prompts

Voinic:

```text
Use case: stylized-concept
Asset type: production source contact sheet for modular 2.5D game character cutouts
Input image 1: style, pixel density, right-facing Romanian folk costume vocabulary, and contact-sheet spacing reference only; create a new Voinic sheet, do not modify the reference
Primary request: Create one complete Romanian Voinic paper-doll source sheet on a perfectly flat solid #00ff00 chroma-key background. Show exactly these separated, non-overlapping right-facing components with generous green gaps: one mature heroic clean-shaven male head in right profile; one separate short swept dark-brown hairstyle silhouette; one cream linen cămașă voinicească torso with restrained red-and-black geometric embroidery and a broad brown leather brâu; two distinct upper-arm sleeves; two distinct forearm sleeves; two distinct bare hands; two distinct dark navy wool upper-leg/cioareci pieces; two distinct dark navy lower-leg/cioareci pieces wrapped with narrow brown leather ties. Arrange as a precise readable grid with no labels.
Style/medium: crisp pixel-adjacent 2.5D hand-painted game art, nearest-neighbor-friendly hard silhouette edges, restrained 96-color feel, consistent with the reference
Composition/framing: isolated components only, all fully visible, right-facing, no overlap, no assembled full body, no equipment, no feet
Lighting/mood: neutral soft upper-left modeling light; consistent across every component
Color palette: cream linen, restrained red and black embroidery, dark navy wool, brown leather, warm skin, dark brown hair
Constraints: one coherent identity and outfit; culturally Romanian; attachment-ready silhouettes; perfectly uniform chroma green background with no floor, shadows, gradients, texture, reflections, text, watermark, frame, or decorations; do not use #00ff00 in any component
Avoid: generic fantasy armor, metal plate, cloak, weapons, hats, full-body sprite, front-facing components, anti-aliased blur, painterly background
```

Ucenic Solomonar:

```text
Use case: stylized-concept
Asset type: production source contact sheet for modular 2.5D game character cutouts
Input image 1: style, pixel density, right-facing Romanian folk costume vocabulary, and contact-sheet spacing reference only; create a new Ucenic Solomonar sheet, do not modify the reference
Primary request: Create one complete Romanian Ucenic Solomonar paper-doll source sheet on a perfectly flat solid #00ff00 chroma-key background. Show exactly these separated, non-overlapping right-facing components with generous green gaps: one youthful thoughtful clean-shaven male head in right profile; one separate medium-length tousled raven-black hairstyle silhouette with a distinctive forelock; one charcoal-grey short wool suman torso over cream linen with restrained white-and-dark-red geometric trim and a narrow oxblood brâu; two distinct charcoal wool upper-arm sleeves; two distinct charcoal wool forearm sleeves with cream cuffs; two distinct bare hands; two distinct cream-grey wool upper-leg/cioareci pieces; two distinct cream-grey wool lower-leg/cioareci pieces wrapped with narrow oxblood leather ties. Arrange as a precise readable grid with no labels.
Style/medium: crisp pixel-adjacent 2.5D hand-painted game art, nearest-neighbor-friendly hard silhouette edges, restrained 96-color feel, consistent with the reference
Composition/framing: isolated components only, all fully visible, right-facing, no overlap, no assembled full body, no equipment, no feet
Lighting/mood: neutral soft upper-left modeling light with a subtle storm-scholar mood; consistent across every component
Color palette: charcoal wool, cream-grey wool and linen, restrained dark red and white geometric trim, oxblood leather, warm olive skin, raven hair
Constraints: one coherent identity and outfit; culturally Romanian; attachment-ready silhouettes; no wizard robe and no generic fantasy symbols; perfectly uniform chroma green background with no floor, shadows, gradients, texture, reflections, text, watermark, frame, or decorations; do not use #00ff00 in any component
Avoid: generic fantasy armor, metal plate, pointy wizard hat, stars, moons, runes, cloak, weapons, full-body sprite, front-facing components, anti-aliased blur, painterly background
```

The generated originals were saved by the built-in tool at:

- `/Users/ioachimlihor/.codex/generated_images/019f7912-ef3e-77c0-b5a1-839d2a40b78f/exec-7e4f7d13-0c59-4a01-a62e-e4d772c2b53d.png`
- `/Users/ioachimlihor/.codex/generated_images/019f7912-ef3e-77c0-b5a1-839d2a40b78f/exec-22a38c73-6a55-4cbb-8f9b-0b6907225c37.png`

Generated-original SHA-256 values:

- `voinic-chroma-master.png`: `a0d08ce9a9bb17156fead38989380d0f119fa39bab9392146a741b900c20864c`
- `ucenic-solomonar-chroma-master.png`: `22b7dddf28fe37d946e153c3efe489e42a87cbb38fb0f0b5b998481948ad14bb`

The tracked masters use the same sensible 128-colour indexed quantization as
v1 to remain below the repository's 1 MiB source-asset policy. Tracked SHA-256
values:

- `voinic-chroma-master.png`: `99ec064c87bcf30df1dcdd815a01c63abd30edbe43c69c52a52108d31e2f41de`
- `ucenic-solomonar-chroma-master.png`: `14bdbf15ba08a7d85f127d36a92d432c9a58087ea67e7acc4efb7683c6c5ddf7`

`scripts/extract-romanian-paper-doll-v1.py` verifies those hashes, applies the
frozen chroma-key/despill recipe, crops the accepted drawings at recorded
source coordinates, quantizes and scales with nearest-neighbour sampling, and
derives exact-alpha mask, normal, and shadow companions. `--check` compares
every generated byte with the tracked runtime files.

The clothing vocabulary follows the project's Romanian reference grammar:
linen cămașă, wool suman and cioareci, brâu, opinci, restrained geometric
embroidery, and practical wrapped lower legs. The presets deliberately avoid
generic fantasy plate and costume mixing. Rights are the same as project
assets unless superseded.
