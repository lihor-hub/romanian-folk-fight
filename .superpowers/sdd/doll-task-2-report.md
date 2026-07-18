# Task 2 report — two-look Romanian production library

## Outcome

DONE. The catalog now resolves the exact Haiduc and Cioban selections, all 15
visible rig attachments use curated production albedos with exact-alpha hybrid
companions, the five requested existing equipment items use the new production
art without gameplay-ID/stat changes, and the gallery prints both looks in
normal and true pixel-mirrored facings.

## Built-in source provenance and rights

Both production masters were generated with OpenAI built-in image generation
for this project. The accepted albedos, deterministic derivatives, and tracked
contact source use “Same as project assets unless superseded.” Cultural research
guided the grammar only; no museum image or other game's pixels were copied.

- Human master: `/Users/ioachimlihor/.codex/generated_images/019f701f-492d-70e1-9c8a-e64740c1d407/exec-0249e3ae-514e-4c04-aef9-9721358f2ddf.png`
- Human SHA-256: `8518d470aae529898a79ed22303cb97b4d358b13fa7bd80088dfac2c0d7334d7`
- Equipment master: `/Users/ioachimlihor/.codex/generated_images/019f701f-492d-70e1-9c8a-e64740c1d407/exec-51bc0f86-e059-429c-89ba-56447e74e048.png`
- Equipment SHA-256: `84b35457846ed8b7e8707e2aedf135cb2a94b3c4b900648307c7215b26f8e0a9`

The exact human and equipment prompts are preserved verbatim under “Exact
human prompt” and “Exact equipment prompt” in
`assets/fighters/human/source/romanian-paper-doll-v1/README.md`. That record
also links the Romanian National Heritage Institute altiță record and two
INP/cIMeC port-popular collection records used for ie/altiță, ițari, cioareci,
opinci, cojoc, and căciulă reference.

The installed imagegen `remove_chroma_key.py` helper ran in built-in mode with
border auto-key, soft matte, thresholds 12/220, and despill. Detected keys were
`#04f807` (human) and `#04f804` (equipment). The tracked 128-colour contact
source is 340 KB, below the 1 MB hook, and remains visually useful. Runtime
extraction still verifies and uses the full-resolution originals.

## Extraction map

`scripts/extract-romanian-paper-doll-v1.py` is the exact reproducible map. Its
`PARTS` table records every source rectangle, accepted output, output size,
semantic mask regions, and face/hair treatment:

- shared: two distinct right-facing side-view opinci at
  `(410,779,502,903)` and `(539,776,638,904)`; short hair from
  `(101,743,216,825)` with skin/face pixels removed;
- Haiduc: face `(71,67,217,314)`, loose hair `(240,67,358,327)`, torso
  `(381,72,595,333)`, two selected sleeve assemblies at x `608..691` and
  `707..789`, two standalone hands, two selected thigh tops at x `984..1086`
  and `1102..1203`, and wrapped shin bottoms at x `1229..1312` and
  `1343..1424`;
- Cioban: face `(72,404,210,655)`, tied hair `(259,406,359,669)`, torso
  `(386,420,594,678)`, two selected sleeve assemblies at x `607..691` and
  `706..789`, two standalone hands, dark-wool thigh tops at x `970..1082`
  and `1092..1201`, and wrapped shin bottoms at x `1235..1319` and
  `1349..1439`;
- equipment: staff `(83,98,219,829)`, axe `(326,221,519,738)`, cojoc
  `(566,211,942,746)`, căciulă `(998,290,1298,636)`, and pair-opinci
  `(1353,512,1701,738)` — exactly five accepted components.

The script splits only two coherent complete sleeves per look at overlapping
embroidered-cuff seams and two coherent legs at overlapping knee seams. It
rejects each third sleeve/hand candidate, the frontal shoe, and six repeated
feet rather than shipping merged or duplicate attachments. Face polygons remove
the long/braided rear-hair masses before the separate hair layers are composed.

All human outputs are two raster pixels per existing display pixel and retain
the legacy attachment pivots/display boxes. Equipment retains the exact existing
attachment points, pivots, and display boxes. Nearest scaling/96-colour curation
keeps the outline and motifs crisp. The accepted albedo alpha is copied byte for
byte into every mask, normal, and shadow. RGB mask positions expose only skin,
hair, cloth, embroidery, and leather; normals remain near flat blue and shadows
remain light local-depth signals.

## Catalog and gallery

Stable IDs added/resolved:

- body: `human.body.zvelt.v1`, `human.body.vanjos.v1`;
- face: `human.face.haiduc.v1`, `human.face.cioban.v1`;
- hair: `human.hair.plete.v1`, `human.hair.prins.v1`, `human.hair.scurt.v1`;
- wardrobe: `human.torso.ie_altita.v1`, `human.legs.itari.v1`,
  `human.torso.camasa_ciobaneasca.v1`, `human.legs.cioareci.v1`,
  `human.feet.opinci.v1`.

`composition.human.haiduc` and `composition.human.cioban` resolve semantic IDs
through catalog v3 rather than sweeping an asset directory. Each contains
exactly 15 albedo layers and prints the six exact selected stable IDs. Technical
maps remain individually reviewable but never enter compositions or gear pages.
The gallery now mirrors both layer position and the layer pixels.

## Red-green evidence

- RED `cargo test --lib character::catalog`: failed on missing
  `human.body.zvelt.v1`.
- RED `cargo test -p xtask assets::gallery`: failed on missing exact catalog
  layers and absent `composition.human.haiduc.html`.
- GREEN catalog test: 24 passed, including exact bundle validation and both
  complete resolutions.
- GREEN gallery test: 33 passed, including exact semantic compositions,
  normal/mirrored specimens, technical-map exclusion, and pixel mirroring.

## Validation and visual review

- `python3 scripts/extract-romanian-paper-doll-v1.py --check` — pass: 34
  albedos, 102 exact-alpha companions, one source sheet, three manifests are
  byte-identical.
- `cargo xtask assets check` — pass: 306/306 files covered; all sidecars,
  catalog references, dimensions, alpha, pivots, and credits clean.
- `cargo xtask assets review` — pass: 302 pages generated, including both exact
  look compositions.
- `cargo test --lib character::catalog` — 24 passed.
- `cargo test --lib character::material` — 5 passed.
- `cargo test --lib cutout::` — 21 passed, including nested elbow/wrist/knee/
  ankle attachment and normal/mirrored rig behavior.
- `cargo test -p xtask assets::gallery` — 33 passed.
- `cargo fmt --all -- --check` — pass.
- `cargo clippy --all-targets -- -D warnings` — pass.
- `cargo test` — 675 passed.
- `git diff --check` — pass.

Headless Chrome review at 1440×900 inspected both 192-pixel-tall gallery
specimens and their mirrored partners. All 15 layers are present once per
look; face/hair silhouettes do not repeat the rejected source head mass;
embroidered cuff and knee seams overlap cleanly; front/back hands and feet are
distinct; ie/altiță, ițari, dark cioareci, opinci ties, and restrained upper-left
light remain readable. The generic static gallery intentionally omits rig
rotation, while the separate cutout tests prove the live nested joint overlap.

## Scope review

No item IDs, stats, prices, slots, attachment points, pivots, display boxes,
character-definition version, non-human art, creation behavior, seeded
generation, review telemetry, or browser baselines changed. The untracked
`.superpowers/brainstorm/` directory predates this task and is excluded from
the commit.
