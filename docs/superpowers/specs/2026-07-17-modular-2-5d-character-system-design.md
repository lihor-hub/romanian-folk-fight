# Modular 2.5D Character System Design

## Decision

Romanian Folk Fight will use one data-driven paper-doll character system for
player customization, procedural NPC generation, named opponents, and folklore
creatures. The canonical art direction is hybrid 2.5D pixel art: high-resolution
layered masters are processed into a coherent pixel-art finish, while normal
maps, directional light, restrained shadows, and joint overlap add volume.

The game remains a side-view fighter. This work does not introduce full 3D
models, a free camera, or physically simulated animation.

## Goals

- Give the player and generated NPCs the same breadth of customization.
- Preserve distinctive authored identities for named fighters and bosses.
- Keep every generated character visually rooted in Romanian traditional dress
  and folklore.
- Reuse the existing Bevy child-entity cutout rig, equipment attachments, and
  pose systems rather than replacing the renderer wholesale.
- Make generated characters deterministic, saveable, and identical in creator
  previews, shops, versus screens, and combat.

## Current State

`src/cutout.rs` already renders articulated body-part children, nested limb
joints, equipped gear layers, mirrored opponents, and human, enemy, and boss
templates. `src/arena/animation.rs` applies eight jointed combat poses. Player
appearance in `src/character/mod.rs` currently exposes four skin tones, builds,
hair styles, and accent colors.

Customization is still shallow: most choices tint, resize, or reposition a
small fixed set of PNGs. Asset paths, skeleton templates, part kinds, and pose
deltas are largely compiled into Rust. Faces, facial hair, garment families,
regional patterns, and anatomical variants are not independently composable.

## Character Definition

Every visible fighter is described by the same save-friendly
`CharacterDefinition`, regardless of how it was produced. It contains:

- a stable schema version and optional generation seed;
- skeleton family and body proportions;
- selected body, face, hair, facial-hair, wardrobe, and accessory parts;
- palette, embroidery, and material selections;
- equipped item visuals;
- cultural profile tags;
- locked identity traits for named encounters.

Players edit this definition through character creation. Procedural NPCs build
it from a seed and generation profile. Named opponents load curated definitions
that may permit limited secondary variation. All consumers pass the completed
definition to the same renderer.

## Skeleton Families and Parts

`SkeletonFamily` defines anatomy and supported attachment slots. Initial
families are human, undead humanoid, beast humanoid, giant, and a specialized
multi-headed boss family. Each family supplies a rest rig and pose set while
sharing the renderer and animation interfaces.

Part catalogs move from fixed Rust match tables into validated asset manifests.
Each part record contains:

- stable ID, source, provenance, and cultural inspiration;
- compatible skeleton families and body regions;
- attachment point, pivot, bounds, and draw layer;
- supported palette-mask and material channels;
- compatible proportion range;
- required tags, exclusions, and companion parts;
- procedural-generation weight and rarity;
- optional age, wear, damage, or supernatural variants.

The runtime resolves manifests into typed registries. Gameplay code references
stable IDs and semantic slots, not filesystem paths.

## Romanian Folk Art Grammar

Romanian identity is a generation constraint, not a cosmetic theme applied
after composition. Assets and profiles use a coherent pan-Romanian folkloric
remix with regional tags rather than strict historical simulation.

Wardrobe families draw from traditional forms such as the ie, ițari, cioareci,
catrință, fotă, brâu, chimir, cojoc, suman, opinci, căciulă, and traditional
head coverings. Weapons favor culturally appropriate or folklore-compatible
forms such as the bâtă ciobănească, baltag, paloș, buzdugan, bow, and spear.

Generation profiles combine compatible region, social role, profession, age,
wealth, and supernatural identity. Folklore creatures may distort or corrupt
traditional clothing, but must remain visibly part of the same world. Generic
medieval-fantasy assets are accepted only after deliberate adaptation to this
visual language.

Every accepted asset records its cultural reference and rights provenance.

## Hybrid 2.5D Rendering

Source artwork is authored as layered, high-resolution masters. Runtime assets
retain a deliberate pixel-art finish and consistent outline, light direction,
pixel density, and palette. Each visible layer may provide:

- albedo with recolorable mask channels;
- a restrained normal map;
- optional roughness or highlight classification;
- an authored depth offset and contact-shadow profile.

The Bevy renderer continues to compose body-part child entities. A shared
character material applies directional lighting, palette masks, and restrained
highlights. Soft contact shadows and overlap at joints provide depth without
making the fighter look like an unrelated 3D model. Pixel snapping and the
project's target display scale remain explicit presentation rules.

Reduced-motion and low-quality modes may simplify animated lighting and shadows
without changing the selected parts or silhouette.

## Generation and Validation

Generation follows this order:

1. Select a skeleton family and legal proportion range.
2. Select a cultural profile: region, role, status, and supernatural identity.
3. Resolve compatible anatomy, face, hair, wardrobe, and accessory parts.
4. Apply compatible palette, embroidery, material, age, and wear variants.
5. Equip legal weapons and armour.
6. Validate requirements, exclusions, attachments, and pose bounds.
7. Persist the definition and seed.

The same seed, catalog version, and generation profile must produce the same
definition. Save files persist resolved stable IDs as the authority; the seed
supports provenance and regeneration diagnostics rather than silently changing
an existing character after a catalog update.

Invalid authored or generated combinations fail validation during development
and asset CI. At runtime, unavailable content resolves to a versioned known-good
archetype and emits a diagnostic. Missing, detached, or invisible parts must not
reach the player.

## Authored Identity and Procedural Variety

Named fighters lock their silhouette-defining traits: skeleton, proportions,
signature head or face, core wardrobe, palette, and signature equipment.
Secondary traits such as wear, small accessories, facial-hair variation, or
minor embroidery may vary when explicitly allowed.

Generic NPC profiles expose wider weighted pools. The player sees the same
compatible catalog but chooses parts directly. Unlock and economy rules may
limit availability without creating a second rendering or data model.

## Delivery Order

1. Define the versioned character schema, registries, and compatibility rules.
2. Deliver one complete human tracer bullet through creator choice, seeded NPC
   generation, save/load, previews, combat, and validation.
3. Add the hybrid 2.5D character material and representative albedo, mask,
   normal, and shadow assets.
4. Establish and review the Romanian wardrobe, anatomy, face, hair, and pattern
   library before broad asset production.
5. Convert named human encounters to curated definitions.
6. Add non-human skeleton families and convert folklore encounters and bosses.
7. Expand combination validation, asset galleries, and visual regression
   coverage.
8. Remove the legacy full-frame fighter sprite path only after feature and
   rendering parity is demonstrated.

## Verification

- Unit tests cover schema migration, compatibility, deterministic generation,
  identity locks, fallback behavior, and stable-ID resolution.
- Headless ECS tests prove attachments, mirroring, equipment inheritance, and
  pose compatibility for every skeleton family.
- Save tests prove a resolved character survives round trips and catalog-version
  changes without changing identity.
- Asset tooling renders representative and adversarial combinations in both
  facings and every supported pose.
- Browser visual tests cover creation, shop or versus preview, and combat at
  desktop and phone sizes, including reduced-motion and rendering fallbacks.
- Human art review approves cultural coherence, silhouette readability, and the
  hybrid 2.5D treatment before each broad production batch.

## Explicitly Out of Scope

- Full 3D meshes, skeletal 3D animation, free camera movement, and real-time
  character sculpting.
- Unrestricted mixing across incompatible skeleton or cultural profiles.
- Generative AI creating new assets during gameplay.
- Rewriting combat rules as part of the rendering migration.
