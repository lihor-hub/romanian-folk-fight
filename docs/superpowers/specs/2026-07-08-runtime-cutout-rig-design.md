# Runtime Cutout Rig Design

## Decision

Issue #89 should introduce a small runtime cutout-rig path without deleting the
existing sprite-sheet fighter path. Fighters gain a shared rig renderer that
spawns visible body-part child entities from explicit part metadata. The first
slice uses simple placeholder sprites in headless/runtime code so the ECS
wiring, transform model, and creator/arena integration are proven before the
production source sheets are sliced into individual transparent PNG files.

## Scope

In scope:

- A reusable Bevy plugin/module for cutout rig data and part spawning.
- A human template for the player and a distinct enemy template for the arena.
- Explicit part names, local offsets, sizes, draw order, and pivot-equivalent
  transforms for torso, head, upper/lower arms, hands, upper/lower legs, and
  feet.
- A creator preview that uses the same rig spawning path as the arena.
- Headless tests for the rig metadata, body-part children, mirroring, and scene
  integration without requiring asset loading.

Out of scope:

- Slicing the generated source sheets into final runtime PNG parts.
- Replacing all combat animation clips with jointed pose animation.
- Moving gear overlays onto true body-part attachments; current gear motion can
  continue until issue #92.

## Architecture

Create a focused `src/cutout.rs` module with:

- `CutoutRigPlugin` for registering any rig systems/resources.
- `CutoutTemplate`, `CutoutPart`, `CutoutPartKind`, and `CutoutRig` data types.
- `spawn_cutout_rig(commands, entity, template, asset_server, flip_x)` to attach
  body-part children to an existing fighter or preview entity.
- `CutoutPartMarker` components on spawned children so tests and later systems
  can address individual parts.

The arena keeps spawning gameplay fighters through `spawn_fighter`, then adds
the cutout rig to those fighter entities instead of the one-atlas body sprite
for the first cutout slice. Existing `FighterClip` and `SpriteAnimation`
components remain on the root fighter so combat events, lunges, footwork, and
return-to-idle logic still run unchanged. Gear overlays continue to attach to
the root fighter for now.

The creator screen adds a small visual preview entity under the existing UI,
then calls the same rig spawning helper with the human template. This proves the
shared rendering path without expanding character customization in this issue.

## Rendering Model

Each body part is a child sprite with a stable local transform:

- Position is the authored attachment/pivot-equivalent offset from the root.
- Rotation starts at the template's neutral pose.
- Scale mirrors on X when `flip_x` is true, so right-facing source art can be
  reused for opponents.
- Z offsets define draw order: rear limbs behind torso, torso in the middle,
  head/front limbs above it.

Runtime art paths are optional in this slice. When an `AssetServer` exists, part
metadata may point at placeholder or future transparent PNG part files. In
headless tests, the spawner emits colored placeholder sprites with the same
sizes and transforms.

## Testing

Tests should prove:

- The human and enemy templates contain the required part set with stable draw
  order.
- Spawning a cutout rig creates one child per part with `CutoutPartMarker`.
- Mirrored rigs invert X offsets but preserve Y and draw order.
- Entering the creator screen spawns a cutout preview.
- Entering the arena spawns player and enemy fighters with cutout rigs while
  keeping combat animation state on the root entities.

## Self-Review

- The design follows the merged pixel-art cutout direction and does not revive
  the old Flash/vector wording.
- The first implementation slice is narrow enough for one PR.
- Existing combat flow and gear presentation are preserved while the new rig
  renderer is introduced.
- Production asset slicing is intentionally deferred, matching the source-sheet
  README files.
