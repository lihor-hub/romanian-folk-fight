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

## Modular character catalog

The human tracer bullet is authored in
`assets/fighters/catalog/human-foundation.json`. It is runtime-readable
character metadata, separate from the media sidecars: the catalog sidecar
intentionally ignores the JSON while each referenced PNG remains owned by its
runtime asset manifest. A catalog record selects one semantic character region,
not an arbitrary full-frame overlay.

### Add a part

1. Add the right-facing transparent PNG to the appropriate runtime asset
   directory. Register it in that directory's `manifest.toml` with its
   provenance, exact license text, dimensions, sampler, attachment metadata,
   and `assets/CREDITS.md` entry before referencing it from the character
   catalog.
2. Add one `parts` record to the catalog with a new, non-blank stable `id`,
   `region`, asset-relative `asset_path`, compatible `skeletons`,
   `cultural_tags`, and `attachment` (`point`, rig-space `pivot`, and
   `draw_layer`). `exclusions` and `companions` are optional stable part-ID
   lists. The catalog document, part-record, and attachment parsers reject
   unknown fields, so change the typed catalog schema and its version before
   introducing a new field.
3. Use a `region` matching the selection slot: `body`, `face`, `hair`,
   `facial_hair`, `torso`, `legs`, `feet`, `waist`, or `accessory`. A valid
   human catalog must provide `body`, `face`, `hair`, `torso`, `legs`, and
   `feet`; `facial_hair`, `waist`, and `accessory` remain optional.
4. Set `attachment.point` to the exact existing cutout part it can replace
   (for example `head`, `hair`, `torso`, `thigh_front`, or `foot_front`). The
   compatibility adapter preserves existing pose transforms and uses a catalog
   PNG only when that point names the rendered rig part. One selected `body`
   supplies both arm chains and one selected `legs` supplies both leg chains.

Stable IDs are saved in `CharacterDefinition`; never rename or reuse one to
mean different content. Add a new versioned ID instead, then migrate saved
definitions deliberately if that is required.

### Cultural compatibility and validation

`cultural_tags` are open-ended authored strings; use lowercase names. A part is eligible
when it shares at least one tag with the definition's cultural profile; it is
not enough that the skeleton and region match. Keep `romanian` on the shared
human foundation and add specific tags such as `chimir`, `itari`, or a future
regional/role tag to describe the part's cultural grammar. Profiles may include
several tags, but every selected part must match at least one of them. Use
`companions` for parts that must be selected together and `exclusions` for
incompatible combinations; all referenced IDs must exist.

After an edit, run the media and catalog checks together:

```bash
cargo xtask assets check
cargo test --lib character::catalog
cargo test --lib character::generation
```

The asset command verifies sidecars, credits, runtime references, image
dimensions, alpha integrity, and attachment metadata. The catalog JSON is
runtime metadata rather than a media record, so the focused catalog and
generation tests validate its typed schema, required regions, cultural
compatibility, relationships, and deterministic selection.

### Review a resolved character

Run `cargo xtask assets review` after changing art or attachment metadata and
open the printed `target/xtask-artifacts/asset-gallery/index.html`. Inspect the
right-facing and mirrored part pages plus the human composition for silhouette,
pixel finish, pivot, draw layer, and gear overlap.

For a generated identity, the review build publishes the representative
opponent's `encounter_id`, derived `seed`, and semantic-order
`resolved_part_ids` in the `generated_opponent` field of the
`rff_review_motion_v1` local-storage snapshot while the fight is active. Run
`cargo xtask web-smoke --scenario gold-journey` to exercise that review build
through creation and combat, or run the focused review assertion:

```bash
cargo test --features review --lib \
  review::tests::generated_opponent_snapshot_exposes_seed_and_resolved_stable_ids
```

Compare those stable IDs with the intended profile and catalog records; do not
approve a look from a screenshot alone.

### Known-good human fallback

Development validation should return catalog and generation errors rather than
silently changing a requested identity. When a safe specimen is needed, build
the catalog's explicit fallback with `character::fallback_human(&catalog)` and
resolve that returned `CharacterDefinition`; it is the versioned
`known_good_human` selection and its matching cultural tags from the catalog.

The scene adapter `spawn_character_definition_rig` invokes the same policy for
persisted human definitions: an unresolvable definition renders the catalog's
known-good human, emits a diagnostic, and leaves the saved definition unchanged
for repair. If the bundled catalog itself cannot be read or its fallback cannot
resolve, it retains the pre-catalog human cutout template for that frame rather
than blocking a scene transition.

### Stable handoff for material and wardrobe phases

The next phases consume `CharacterDefinition` and resolved stable `PartId`s,
not filesystem paths. Preserve the definition's version, optional provenance
seed, skeleton family, cultural profile, semantic `PartSelections`, and
temporary `PlayerAppearance` bridge; a saved resolved selection remains the
identity authority even when a seed can explain how it was generated.

The existing cutout semantic kinds, rest-pose hierarchy, `CutoutPartMarker`
`source_id`, and gear attachment layers remain the pose and equipment
contracts. Material work should add rendering data without changing those
selected IDs or silhouettes, and wardrobe work should add compatible catalog
records and profiles without creating a second character format. New skeleton
families, material channels, and richer wardrobe metadata require a deliberate
catalog/schema version rather than unrecognized JSON fields.

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
