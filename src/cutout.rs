//! Runtime cutout-rig rendering: small, explicit body-part templates that can
//! be attached to creator previews and arena fighter roots. This slice wires
//! the first production-intent pixel-art body parts and starter gear into the
//! runtime while keeping the ECS surface compact.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::character::{AccentColor, BodyBuild, HairStyle, PlayerAppearance, SkinTone};
use crate::items::{Equipment, GearAttachment, GearMotion, ItemId, ItemVisual, Slot, item_visual};

/// Registers cutout-rig support. The first implementation is spawn-helper
/// driven, so the plugin currently documents ownership without scheduling
/// systems.
pub struct CutoutRigPlugin;

impl Plugin for CutoutRigPlugin {
    fn build(&self, _app: &mut App) {}
}

/// Which authored rig template a root entity carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutoutTemplate {
    Human,
    Enemy,
    Boss,
}

/// Stable body-part identifiers. Front/back variants give the first rig a
/// usable draw order while staying close to the planned anatomical part set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CutoutPartKind {
    Hair,
    UpperArmBack,
    ForearmBack,
    HandBack,
    ThighBack,
    ShinBack,
    FootBack,
    Torso,
    Head,
    UpperArmFront,
    ForearmFront,
    HandFront,
    ThighFront,
    ShinFront,
    FootFront,
}

/// Static metadata for one visible body part.
#[derive(Debug, Clone, PartialEq)]
pub struct CutoutPart {
    pub kind: CutoutPartKind,
    /// Local translation. For a chained part (a forearm, hand, shin, or
    /// foot -- see [`parent_kind`]) this is relative to its parent PART's
    /// own origin, not the rig root, and becomes that part's local
    /// [`Transform`] translation once nested under it. Root-level parts
    /// (torso, head, hair, upper arms, thighs) stay root-relative, as
    /// before.
    pub offset: Vec2,
    /// This part's own attachment point for the next link in its joint
    /// chain (e.g. the elbow on an upper arm, the wrist on a forearm, the
    /// knee on a thigh, the ankle on a shin), expressed as an offset from
    /// this part's own `offset` in the same (unrotated, parent-relative)
    /// space `offset` uses. `Vec2::ZERO` for parts with no rig child
    /// (hands, feet, torso, head, hair). Documents the pivot/attachment
    /// point `docs/art-direction.md` requires, and is what makes a nested
    /// child's `offset` follow its parent's rotation through Bevy's
    /// transform propagation instead of drifting into a gap (#117).
    pub pivot: Vec2,
    pub size: Vec2,
    /// Local rotation. For a chained part this composes with the parent
    /// part's own rotation through Bevy's transform propagation, so it is
    /// relative to the parent's rotation rather than an absolute angle (see
    /// `offset`).
    pub rotation: f32,
    pub z_offset: f32,
    pub color: Color,
    pub asset_path: Option<&'static str>,
}

impl CutoutPart {
    /// Sets this part's [`pivot`](CutoutPart::pivot) attachment point.
    fn with_pivot(mut self, x: f32, y: f32) -> Self {
        self.pivot = Vec2::new(x, y);
        self
    }
}

/// Which body part `kind` is rigged to, if any. Forearms hang from upper
/// arms, hands from forearms, shins from thighs, and feet from shins;
/// Bevy's transform hierarchy propagates the parent's rotation to the child
/// automatically, which is what keeps a joint from opening a gap when the
/// limb rotates (#117). Every other part (torso, head, hair, upper arms,
/// thighs) is parented directly to the rig root, as before.
fn parent_kind(kind: CutoutPartKind) -> Option<CutoutPartKind> {
    use CutoutPartKind::*;
    match kind {
        ForearmBack => Some(UpperArmBack),
        ForearmFront => Some(UpperArmFront),
        HandBack => Some(ForearmBack),
        HandFront => Some(ForearmFront),
        ShinBack => Some(ThighBack),
        ShinFront => Some(ThighFront),
        FootBack => Some(ShinBack),
        FootFront => Some(ShinFront),
        Hair | Torso | Head | UpperArmBack | UpperArmFront | ThighBack | ThighFront => None,
    }
}

/// Direct rig children of `kind`, if any -- the inverse of [`parent_kind`].
fn child_kinds(kind: CutoutPartKind) -> &'static [CutoutPartKind] {
    use CutoutPartKind::*;
    match kind {
        UpperArmBack => &[ForearmBack],
        UpperArmFront => &[ForearmFront],
        ForearmBack => &[HandBack],
        ForearmFront => &[HandFront],
        ThighBack => &[ShinBack],
        ThighFront => &[ShinFront],
        ShinBack => &[FootBack],
        ShinFront => &[FootFront],
        _ => &[],
    }
}

/// Walks up the cutout-rig hierarchy from `entity`, following `parent_of`
/// until it returns `None`. Chained limb parts (forearms, hands, shins,
/// feet) are nested several joints deep rather than being direct children
/// of the rig root, so any system that needs "which fighter owns this part"
/// (gear attachment/sync, pose application) must climb the chain instead of
/// assuming a single hop. `parent_of` should return `Some(parent)` while
/// `entity` is itself a rigged body part, and `None` once it reaches the
/// owning root (or anything else that isn't part of the chain).
pub fn cutout_rig_owner(entity: Entity, parent_of: impl Fn(Entity) -> Option<Entity>) -> Entity {
    const MAX_DEPTH: usize = 8;
    let mut current = entity;
    for _ in 0..MAX_DEPTH {
        match parent_of(current) {
            Some(parent) => current = parent,
            None => return current,
        }
    }
    current
}

/// A complete neutral-pose rig template.
#[derive(Debug, Clone, PartialEq)]
pub struct CutoutRigTemplate {
    pub template: CutoutTemplate,
    pub parts: Vec<CutoutPart>,
}

/// Marker on a root entity rendered through the cutout path.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutoutRig {
    pub template: CutoutTemplate,
    pub flip_x: bool,
}

/// Marker on each body-part child.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutoutPartMarker {
    pub kind: CutoutPartKind,
}

/// Current jointed pose applied to a cutout rig root by arena presentation.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutoutPose {
    Idle,
    Attack,
    Block,
    Dodge,
    HitReaction,
    Knockdown,
    StepForward,
    StepBack,
}

/// Neutral transform data for a body part. Pose systems always rebuild from
/// this rest pose, so repeated combat events cannot accumulate drift.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CutoutPartRestPose {
    pub transform: Transform,
    pub size: Vec2,
}

/// One visible equipment layer attached to a cutout body part.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct GearVisualLayer {
    /// Catalog item shown by this layer.
    pub item: ItemId,
    /// Equipment slot this layer occupies.
    pub slot: Slot,
    /// Rig body part this layer follows.
    pub attachment: GearAttachment,
    /// Motion profile used by arena animation synchronizers.
    pub motion: GearMotion,
    /// Stable draw order relative to the owning body part.
    pub z_offset: f32,
}

/// Human/player neutral pose.
pub fn human_template() -> CutoutRigTemplate {
    human_template_for(PlayerAppearance::default())
}

/// Human/player neutral pose customized for one saved appearance selection.
pub fn human_template_for(appearance: PlayerAppearance) -> CutoutRigTemplate {
    let mut parts = human_parts(1.0);
    apply_player_appearance(&mut parts, appearance);
    CutoutRigTemplate {
        template: CutoutTemplate::Human,
        parts,
    }
}

fn align_rig_joint_offsets(parts: &mut [CutoutPart]) {
    let parent_pivots: HashMap<CutoutPartKind, Vec2> =
        parts.iter().map(|p| (p.kind, p.pivot)).collect();
    for part in parts.iter_mut() {
        if let Some(parent_pivot) = parent_kind(part.kind).and_then(|k| parent_pivots.get(&k)) {
            part.offset = *parent_pivot;
        }
    }
}

/// First enemy neutral pose. It reuses the same part categories with leaner,
/// slightly taller proportions to prove non-player use of the path.
pub fn enemy_template() -> CutoutRigTemplate {
    let mut parts: Vec<CutoutPart> = human_parts(1.0).into_iter().map(strigoi_part).collect();
    align_rig_joint_offsets(&mut parts);
    CutoutRigTemplate {
        template: CutoutTemplate::Enemy,
        parts,
    }
}

/// Larger boss-style neutral pose with broader proportions and a taller
/// silhouette so major enemies read differently at a glance.
pub fn boss_template() -> CutoutRigTemplate {
    let mut parts: Vec<CutoutPart> = human_parts(1.26).into_iter().map(zmeu_part).collect();
    align_rig_joint_offsets(&mut parts);
    CutoutRigTemplate {
        template: CutoutTemplate::Boss,
        parts,
    }
}

/// Attaches a cutout rig to `root`, preserving the root as the gameplay and
/// animation entity.
pub fn spawn_cutout_rig(
    commands: &mut Commands,
    root: Entity,
    template: CutoutRigTemplate,
    asset_server: Option<&AssetServer>,
    flip_x: bool,
) {
    spawn_cutout_rig_impl(commands, root, template, asset_server, flip_x, None, None);
}

/// Attaches a cutout rig and its currently equipped gear to `root`.
pub fn spawn_cutout_rig_with_gear(
    commands: &mut Commands,
    root: Entity,
    template: CutoutRigTemplate,
    asset_server: Option<&AssetServer>,
    flip_x: bool,
    equipment: &Equipment,
) {
    spawn_cutout_rig_impl(
        commands,
        root,
        template,
        asset_server,
        flip_x,
        Some(equipment),
        None,
    );
}

/// Attaches a cutout rig tinted with one opponent accent hue (#118):
/// `accent_hue`, when given, replaces `Sprite::color` on the rig's
/// clothing/accent parts only (see [`is_accent_part`]) -- never skin,
/// outline/footwear, or hair -- so an opponent reads as visually distinct
/// from the untinted player template it may otherwise share pixel-for-pixel.
/// `None` leaves every part's sprite at its default (untinted) color, same
/// as [`spawn_cutout_rig`].
pub fn spawn_cutout_rig_with_accent(
    commands: &mut Commands,
    root: Entity,
    template: CutoutRigTemplate,
    asset_server: Option<&AssetServer>,
    flip_x: bool,
    accent_hue: Option<Color>,
) {
    spawn_cutout_rig_impl(
        commands,
        root,
        template,
        asset_server,
        flip_x,
        None,
        accent_hue,
    );
}

fn spawn_cutout_rig_impl(
    commands: &mut Commands,
    root: Entity,
    template: CutoutRigTemplate,
    asset_server: Option<&AssetServer>,
    flip_x: bool,
    equipment: Option<&Equipment>,
    accent_hue: Option<Color>,
) {
    commands.entity(root).insert(CutoutRig {
        template: template.template,
        flip_x,
    });
    commands.entity(root).insert(CutoutPose::Idle);

    // Root-level spawn order follows the template's authored (draw) order,
    // skipping chained parts (forearms, hands, shins, feet): those are
    // spawned recursively as children of their own parent part below, so
    // Bevy's transform hierarchy carries the parent's rotation to them
    // instead of leaving them as independently-offset siblings (#117).
    let root_order: Vec<CutoutPartKind> = template
        .parts
        .iter()
        .map(|part| part.kind)
        .filter(|kind| parent_kind(*kind).is_none())
        .collect();
    let parts_by_kind: HashMap<CutoutPartKind, CutoutPart> = template
        .parts
        .into_iter()
        .map(|part| (part.kind, part))
        .collect();

    commands.entity(root).with_children(|body| {
        for kind in root_order {
            spawn_part_and_children(
                body,
                &parts_by_kind,
                kind,
                flip_x,
                asset_server,
                equipment,
                accent_hue,
            );
        }
    });
}

/// Spawns `kind`'s body-part entity and, recursively, every part rigged
/// beneath it (see [`child_kinds`]) as its Bevy children -- and, if
/// `equipment` is given, any gear layers attached to each of those parts.
#[allow(clippy::too_many_arguments)]
fn spawn_part_and_children(
    parent: &mut ChildSpawnerCommands,
    parts_by_kind: &HashMap<CutoutPartKind, CutoutPart>,
    kind: CutoutPartKind,
    flip_x: bool,
    asset_server: Option<&AssetServer>,
    equipment: Option<&Equipment>,
    accent_hue: Option<Color>,
) {
    let Some(part) = parts_by_kind.get(&kind) else {
        return;
    };
    let transform = part_transform(part, flip_x);
    let tint = accent_hue.filter(|_| is_accent_part(kind));
    parent
        .spawn((
            CutoutPartMarker { kind },
            CutoutPartRestPose {
                transform,
                size: part.size,
            },
            part_sprite(part, asset_server, flip_x, tint),
            transform,
        ))
        .with_children(|part_children| {
            if let Some(equipment) = equipment {
                spawn_gear_children_for_part(
                    part_children,
                    kind,
                    equipment,
                    asset_server,
                    &mut |_| (),
                );
            }
            for &child_kind in child_kinds(kind) {
                spawn_part_and_children(
                    part_children,
                    parts_by_kind,
                    child_kind,
                    flip_x,
                    asset_server,
                    equipment,
                    accent_hue,
                );
            }
        });
}

/// Which cutout parts count as "clothing/accent" for the #118 opponent tint:
/// the torso and limb cloth, never skin (head/hands), footwear/outline
/// (feet, which render near-black like the palette's outline color), or
/// hair.
fn is_accent_part(kind: CutoutPartKind) -> bool {
    use CutoutPartKind::*;
    matches!(
        kind,
        Torso
            | UpperArmBack
            | UpperArmFront
            | ForearmBack
            | ForearmFront
            | ThighBack
            | ThighFront
            | ShinBack
            | ShinFront
    )
}

/// Spawns equipped gear under already-existing cutout body part entities.
pub fn spawn_gear_attachment_layers<B: Bundle>(
    commands: &mut Commands,
    equipment: &Equipment,
    asset_server: Option<&AssetServer>,
    mut part_entity: impl FnMut(CutoutPartKind) -> Option<Entity>,
    mut extra_bundle: impl FnMut(&ItemVisual) -> B,
) {
    for slot in Slot::ALL {
        let Some(item) = equipment.equipped(slot) else {
            continue;
        };
        let Some(visual) = item_visual(item) else {
            continue;
        };
        for &attachment_part in visual.attachment.parts {
            let Some(part) = part_entity(attachment_part) else {
                continue;
            };
            commands.entity(part).with_children(|body| {
                body.spawn((
                    gear_layer_bundle(item, visual, asset_server),
                    extra_bundle(visual),
                ));
            });
        }
    }
}

fn spawn_gear_children_for_part<B: Bundle>(
    parent: &mut ChildSpawnerCommands,
    part: CutoutPartKind,
    equipment: &Equipment,
    asset_server: Option<&AssetServer>,
    extra_bundle: &mut impl FnMut(&ItemVisual) -> B,
) {
    for slot in Slot::ALL {
        let Some(item) = equipment.equipped(slot) else {
            continue;
        };
        let Some(visual) = item_visual(item) else {
            continue;
        };
        if !visual.attachment.parts.contains(&part) {
            continue;
        }
        parent.spawn((
            gear_layer_bundle(item, visual, asset_server),
            extra_bundle(visual),
        ));
    }
}

fn gear_layer_bundle(
    item: ItemId,
    visual: &ItemVisual,
    asset_server: Option<&AssetServer>,
) -> (GearVisualLayer, Sprite, Transform) {
    (
        GearVisualLayer {
            item,
            slot: visual.slot,
            attachment: visual.attachment,
            motion: visual.motion,
            z_offset: visual.z_offset,
        },
        gear_sprite(visual, asset_server),
        gear_attachment_transform(visual.offset, visual.z_offset),
    )
}

/// Runtime uses generated transparent PNGs; headless tests without an
/// [`AssetServer`] spawn a harmless placeholder sprite so ECS behavior stays
/// testable.
pub fn gear_sprite(visual: &ItemVisual, asset_server: Option<&AssetServer>) -> Sprite {
    let size = Vec2::new(visual.size.0, visual.size.1);
    if let Some(asset_server) = asset_server {
        Sprite {
            custom_size: Some(size),
            ..Sprite::from_image(asset_server.load(visual.fallback_asset_path()))
        }
    } else {
        Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.35), size)
    }
}

pub fn gear_attachment_transform(offset: (f32, f32), z_offset: f32) -> Transform {
    Transform::from_xyz(offset.0, offset.1, z_offset)
}

/// Builds one body-part sprite. `flip_x` mirrors the artwork itself so a
/// mirrored rig faces its opponent; [`part_transform`] mirrors only the
/// position/rotation. `tint`, when given, becomes this sprite's
/// `Sprite::color` -- the #118 accent-hue wash -- overriding the default
/// (untinted) color regardless of whether the part renders from a loaded
/// image or the headless fallback color.
fn part_sprite(
    part: &CutoutPart,
    asset_server: Option<&AssetServer>,
    flip_x: bool,
    tint: Option<Color>,
) -> Sprite {
    let mut sprite = match (asset_server, part.asset_path) {
        (Some(asset_server), Some(path)) => Sprite {
            custom_size: Some(part.size),
            flip_x,
            ..Sprite::from_image(asset_server.load(path))
        },
        _ => Sprite {
            flip_x,
            ..Sprite::from_color(part.color, part.size)
        },
    };
    if let Some(tint) = tint {
        sprite.color = tint;
    }
    sprite
}

fn part_transform(part: &CutoutPart, flip_x: bool) -> Transform {
    let x = if flip_x {
        -part.offset.x
    } else {
        part.offset.x
    };
    let rotation = if flip_x {
        -part.rotation
    } else {
        part.rotation
    };
    Transform::from_xyz(x, part.offset.y, part.z_offset)
        .with_rotation(Quat::from_rotation_z(rotation))
}

/// Builds the neutral-pose rig. Chained parts (forearms, hands, shins,
/// feet -- see [`parent_kind`]) are authored here against their parent
/// part's [`CutoutPart::pivot`], so `offset`/`rotation`/`z_offset` end up
/// relative to that parent rather than to the rig root: the rest pose stays
/// pixel-identical to the original flat (root-relative) layout, but once
/// [`spawn_part_and_children`] nests them under their parent entity, Bevy's
/// transform propagation carries the parent's rotation to them too, closing
/// the elbow/wrist/knee/ankle gap a bare rotation used to open (#117).
///
/// The `_ROT` constants below are the original absolute (root-relative)
/// rest rotations this rig was authored with; each chained part's relative
/// rotation is `own absolute - parent's absolute`, which telescopes back to
/// the original absolute value once composed through the parent chain. The
/// same trick is used for `z_offset` (draw-depth) so sprite layering is
/// unchanged too.
fn human_parts(scale: f32) -> Vec<CutoutPart> {
    const UPPER_ARM_BACK_ROT: f32 = -0.18;
    const FOREARM_BACK_ROT: f32 = -0.28;
    const HAND_BACK_ROT: f32 = -0.1;
    const THIGH_BACK_ROT: f32 = 0.08;
    const SHIN_BACK_ROT: f32 = -0.05;
    const FOOT_BACK_ROT: f32 = 0.0;
    const UPPER_ARM_FRONT_ROT: f32 = -0.12;
    const FOREARM_FRONT_ROT: f32 = -0.18;
    const HAND_FRONT_ROT: f32 = -0.04;
    const THIGH_FRONT_ROT: f32 = -0.07;
    const SHIN_FRONT_ROT: f32 = 0.04;
    const FOOT_FRONT_ROT: f32 = 0.0;

    let upper_arm_back = part(
        CutoutPartKind::UpperArmBack,
        -20.0,
        26.0,
        15.0,
        44.0,
        UPPER_ARM_BACK_ROT,
        -0.08,
    )
    .with_pivot(-8.0, -28.0);
    let forearm_back = part(
        CutoutPartKind::ForearmBack,
        upper_arm_back.pivot.x,
        upper_arm_back.pivot.y,
        13.0,
        38.0,
        FOREARM_BACK_ROT - UPPER_ARM_BACK_ROT,
        -0.07 - (-0.08),
    )
    .with_pivot(-4.0, -24.0);
    let hand_back = part(
        CutoutPartKind::HandBack,
        forearm_back.pivot.x,
        forearm_back.pivot.y,
        13.0,
        13.0,
        HAND_BACK_ROT - FOREARM_BACK_ROT,
        -0.06 - (-0.07),
    );
    let thigh_back = part(
        CutoutPartKind::ThighBack,
        -13.0,
        -42.0,
        17.0,
        42.0,
        THIGH_BACK_ROT,
        -0.05,
    )
    .with_pivot(-2.0, -34.0);
    let shin_back = part(
        CutoutPartKind::ShinBack,
        thigh_back.pivot.x,
        thigh_back.pivot.y,
        14.0,
        38.0,
        SHIN_BACK_ROT - THIGH_BACK_ROT,
        -0.04 - (-0.05),
    )
    .with_pivot(7.0, -26.0);
    let foot_back = part(
        CutoutPartKind::FootBack,
        shin_back.pivot.x,
        shin_back.pivot.y,
        28.0,
        12.0,
        FOOT_BACK_ROT - SHIN_BACK_ROT,
        -0.03 - (-0.04),
    );
    let torso = part(CutoutPartKind::Torso, 0.0, 6.0, 44.0, 74.0, 0.0, 0.0);
    let hair = part(CutoutPartKind::Hair, 1.0, 71.0, 32.0, 20.0, 0.02, 0.02);
    let head = part(CutoutPartKind::Head, 4.0, 60.0, 38.0, 42.0, 0.04, 0.03);
    let upper_arm_front = part(
        CutoutPartKind::UpperArmFront,
        21.0,
        25.0,
        15.0,
        45.0,
        UPPER_ARM_FRONT_ROT,
        0.04,
    )
    .with_pivot(8.0, -28.0);
    let forearm_front = part(
        CutoutPartKind::ForearmFront,
        upper_arm_front.pivot.x,
        upper_arm_front.pivot.y,
        13.0,
        39.0,
        FOREARM_FRONT_ROT - UPPER_ARM_FRONT_ROT,
        0.05 - 0.04,
    )
    .with_pivot(4.0, -25.0);
    let hand_front = part(
        CutoutPartKind::HandFront,
        forearm_front.pivot.x,
        forearm_front.pivot.y,
        13.0,
        13.0,
        HAND_FRONT_ROT - FOREARM_FRONT_ROT,
        0.06 - 0.05,
    );
    let thigh_front = part(
        CutoutPartKind::ThighFront,
        13.0,
        -42.0,
        17.0,
        42.0,
        THIGH_FRONT_ROT,
        0.07,
    )
    .with_pivot(2.0, -34.0);
    let shin_front = part(
        CutoutPartKind::ShinFront,
        thigh_front.pivot.x,
        thigh_front.pivot.y,
        14.0,
        38.0,
        SHIN_FRONT_ROT - THIGH_FRONT_ROT,
        0.08 - 0.07,
    )
    .with_pivot(8.0, -26.0);
    let foot_front = part(
        CutoutPartKind::FootFront,
        shin_front.pivot.x,
        shin_front.pivot.y,
        28.0,
        12.0,
        FOOT_FRONT_ROT - SHIN_FRONT_ROT,
        0.09 - 0.08,
    );

    vec![
        upper_arm_back,
        forearm_back,
        hand_back,
        thigh_back,
        shin_back,
        foot_back,
        torso,
        hair,
        head,
        upper_arm_front,
        forearm_front,
        hand_front,
        thigh_front,
        shin_front,
        foot_front,
    ]
    .into_iter()
    .map(|mut part| {
        part.offset *= scale;
        part.pivot *= scale;
        part.size *= scale;
        part.asset_path = human_asset_path(part.kind);
        part
    })
    .collect()
}

/// Resolves `kind`'s effective (composed) `z_offset` by walking up through
/// [`parent_kind`] and summing every ancestor's local `z_offset`, mirroring
/// what Bevy's transform propagation actually renders for a chained part.
/// Used by tests to check draw-order the same way the fix's authors did:
/// [`human_parts`] stores chained `z_offset`s relative to their parent so
/// the *composed* depth reconstructs the original absolute authoring, not
/// the raw per-part field.
#[cfg(test)]
fn effective_z_offset(parts: &[CutoutPart], kind: CutoutPartKind) -> f32 {
    let mut total = 0.0;
    let mut current = Some(kind);
    while let Some(k) = current {
        total += parts
            .iter()
            .find(|part| part.kind == k)
            .map(|part| part.z_offset)
            .unwrap_or(0.0);
        current = parent_kind(k);
    }
    total
}

fn strigoi_part(mut part: CutoutPart) -> CutoutPart {
    part.color = enemy_color(part.kind);
    part.asset_path = strigoi_asset_path(part.kind);
    match part.kind {
        CutoutPartKind::Hair => {
            part.color = Color::NONE;
            part.asset_path = None;
            part.offset.x -= 3.0;
            part.offset.y += 13.0;
            part.size.x *= 1.06;
            part.size.y *= 1.3;
            part.pivot.x *= 1.06;
            part.pivot.y *= 1.3;
        }
        CutoutPartKind::Torso => {
            part.offset.y += 6.0;
            part.size.x *= 0.72;
            part.size.y *= 0.94;
            part.pivot.x *= 0.72;
            part.pivot.y *= 0.94;
        }
        CutoutPartKind::Head => {
            part.offset.x -= 4.0;
            part.offset.y += 12.0;
            part.size.x *= 1.18;
            part.size.y *= 1.22;
            part.pivot.x *= 1.18;
            part.pivot.y *= 1.22;
        }
        CutoutPartKind::UpperArmBack | CutoutPartKind::UpperArmFront => {
            part.offset.x *= 0.82;
            part.offset.y += 8.0;
            part.size.x *= 0.86;
            part.size.y *= 1.18;
            part.pivot.x *= 0.86;
            part.pivot.y *= 1.18;
        }
        CutoutPartKind::ForearmBack | CutoutPartKind::ForearmFront => {
            part.offset.x *= 0.8;
            part.offset.y += 4.0;
            part.size.x *= 0.84;
            part.size.y *= 1.22;
            part.pivot.x *= 0.84;
            part.pivot.y *= 1.22;
        }
        CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            part.offset.x *= 0.8;
            part.offset.y += 2.0;
            part.size.x *= 0.9;
            part.size.y *= 1.08;
            part.pivot.x *= 0.9;
            part.pivot.y *= 1.08;
        }
        CutoutPartKind::ThighBack | CutoutPartKind::ThighFront => {
            part.offset.x *= 0.92;
            part.offset.y -= 2.0;
            part.size.x *= 0.9;
            part.size.y *= 1.08;
            part.pivot.x *= 0.9;
            part.pivot.y *= 1.08;
        }
        CutoutPartKind::ShinBack | CutoutPartKind::ShinFront => {
            part.offset.x *= 0.9;
            part.offset.y -= 6.0;
            part.size.x *= 0.86;
            part.size.y *= 1.16;
            part.pivot.x *= 0.86;
            part.pivot.y *= 1.16;
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => {
            part.offset.x *= 0.82;
            part.offset.y -= 8.0;
            part.size.x *= 0.76;
            part.size.y *= 0.9;
            part.pivot.x *= 0.76;
            part.pivot.y *= 0.9;
        }
    }
    part
}

fn zmeu_part(mut part: CutoutPart) -> CutoutPart {
    part.color = boss_color(part.kind);
    part.asset_path = zmeu_asset_path(part.kind);
    match part.kind {
        CutoutPartKind::Hair => {
            part.color = Color::NONE;
            part.asset_path = None;
            part.offset.x += 2.0;
            part.offset.y += 21.0;
            part.size.x *= 1.16;
            part.size.y *= 1.34;
            part.pivot.x *= 1.16;
            part.pivot.y *= 1.34;
        }
        CutoutPartKind::Torso => {
            part.offset.y += 10.0;
            part.size.x *= 1.34;
            part.size.y *= 1.2;
            part.pivot.x *= 1.34;
            part.pivot.y *= 1.2;
        }
        CutoutPartKind::Head => {
            part.offset.x += 4.0;
            part.offset.y += 18.0;
            part.size.x *= 1.14;
            part.size.y *= 1.18;
            part.pivot.x *= 1.14;
            part.pivot.y *= 1.18;
        }
        CutoutPartKind::UpperArmBack | CutoutPartKind::UpperArmFront => {
            part.offset.x *= 1.16;
            part.offset.y += 8.0;
            part.size.x *= 1.42;
            part.size.y *= 1.12;
            part.pivot.x *= 1.42;
            part.pivot.y *= 1.12;
        }
        CutoutPartKind::ForearmBack | CutoutPartKind::ForearmFront => {
            part.offset.x *= 1.16;
            part.offset.y += 4.0;
            part.size.x *= 1.34;
            part.size.y *= 1.14;
            part.pivot.x *= 1.34;
            part.pivot.y *= 1.14;
        }
        CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            part.offset.x *= 1.18;
            part.size.x *= 1.28;
            part.size.y *= 1.2;
            part.pivot.x *= 1.28;
            part.pivot.y *= 1.2;
        }
        CutoutPartKind::ThighBack | CutoutPartKind::ThighFront => {
            part.offset.x *= 1.08;
            part.offset.y -= 4.0;
            part.size.x *= 1.34;
            part.size.y *= 1.12;
            part.pivot.x *= 1.34;
            part.pivot.y *= 1.12;
        }
        CutoutPartKind::ShinBack | CutoutPartKind::ShinFront => {
            part.offset.x *= 1.08;
            part.offset.y -= 10.0;
            part.size.x *= 1.28;
            part.size.y *= 1.1;
            part.pivot.x *= 1.28;
            part.pivot.y *= 1.1;
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => {
            part.offset.x *= 1.12;
            part.offset.y -= 12.0;
            part.size.x *= 1.38;
            part.size.y *= 1.22;
            part.pivot.x *= 1.38;
            part.pivot.y *= 1.22;
        }
    }
    part
}

fn part(
    kind: CutoutPartKind,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation: f32,
    z_offset: f32,
) -> CutoutPart {
    CutoutPart {
        kind,
        offset: Vec2::new(x, y),
        pivot: Vec2::ZERO,
        size: Vec2::new(width, height),
        rotation,
        z_offset,
        color: human_color(kind),
        asset_path: None,
    }
}

fn human_asset_path(kind: CutoutPartKind) -> Option<&'static str> {
    Some(match kind {
        CutoutPartKind::Hair => "fighters/human/runtime/hair.png",
        CutoutPartKind::UpperArmBack => "fighters/human/runtime/upper_arm_back.png",
        CutoutPartKind::ForearmBack => "fighters/human/runtime/forearm_back.png",
        CutoutPartKind::HandBack => "fighters/human/runtime/hand_back.png",
        CutoutPartKind::ThighBack => "fighters/human/runtime/thigh_back.png",
        CutoutPartKind::ShinBack => "fighters/human/runtime/shin_back.png",
        CutoutPartKind::FootBack => "fighters/human/runtime/foot_back.png",
        CutoutPartKind::Torso => "fighters/human/runtime/torso.png",
        CutoutPartKind::Head => "fighters/human/runtime/head.png",
        CutoutPartKind::UpperArmFront => "fighters/human/runtime/upper_arm_front.png",
        CutoutPartKind::ForearmFront => "fighters/human/runtime/forearm_front.png",
        CutoutPartKind::HandFront => "fighters/human/runtime/hand_front.png",
        CutoutPartKind::ThighFront => "fighters/human/runtime/thigh_front.png",
        CutoutPartKind::ShinFront => "fighters/human/runtime/shin_front.png",
        CutoutPartKind::FootFront => "fighters/human/runtime/foot_front.png",
    })
}

fn strigoi_asset_path(kind: CutoutPartKind) -> Option<&'static str> {
    Some(match kind {
        CutoutPartKind::Hair => return None,
        CutoutPartKind::UpperArmBack => "fighters/strigoi/runtime/upper_arm_back.png",
        CutoutPartKind::ForearmBack => "fighters/strigoi/runtime/forearm_back.png",
        CutoutPartKind::HandBack => "fighters/strigoi/runtime/hand_back.png",
        CutoutPartKind::ThighBack => "fighters/strigoi/runtime/thigh_back.png",
        CutoutPartKind::ShinBack => "fighters/strigoi/runtime/shin_back.png",
        CutoutPartKind::FootBack => "fighters/strigoi/runtime/foot_back.png",
        CutoutPartKind::Torso => "fighters/strigoi/runtime/torso.png",
        CutoutPartKind::Head => "fighters/strigoi/runtime/head.png",
        CutoutPartKind::UpperArmFront => "fighters/strigoi/runtime/upper_arm_front.png",
        CutoutPartKind::ForearmFront => "fighters/strigoi/runtime/forearm_front.png",
        CutoutPartKind::HandFront => "fighters/strigoi/runtime/hand_front.png",
        CutoutPartKind::ThighFront => "fighters/strigoi/runtime/thigh_front.png",
        CutoutPartKind::ShinFront => "fighters/strigoi/runtime/shin_front.png",
        CutoutPartKind::FootFront => "fighters/strigoi/runtime/foot_front.png",
    })
}

fn zmeu_asset_path(kind: CutoutPartKind) -> Option<&'static str> {
    Some(match kind {
        CutoutPartKind::Hair => return None,
        CutoutPartKind::UpperArmBack => "fighters/zmeu/runtime/upper_arm_back.png",
        CutoutPartKind::ForearmBack => "fighters/zmeu/runtime/forearm_back.png",
        CutoutPartKind::HandBack => "fighters/zmeu/runtime/hand_back.png",
        CutoutPartKind::ThighBack => "fighters/zmeu/runtime/thigh_back.png",
        CutoutPartKind::ShinBack => "fighters/zmeu/runtime/shin_back.png",
        CutoutPartKind::FootBack => "fighters/zmeu/runtime/foot_back.png",
        CutoutPartKind::Torso => "fighters/zmeu/runtime/torso.png",
        CutoutPartKind::Head => "fighters/zmeu/runtime/head.png",
        CutoutPartKind::UpperArmFront => "fighters/zmeu/runtime/upper_arm_front.png",
        CutoutPartKind::ForearmFront => "fighters/zmeu/runtime/forearm_front.png",
        CutoutPartKind::HandFront => "fighters/zmeu/runtime/hand_front.png",
        CutoutPartKind::ThighFront => "fighters/zmeu/runtime/thigh_front.png",
        CutoutPartKind::ShinFront => "fighters/zmeu/runtime/shin_front.png",
        CutoutPartKind::FootFront => "fighters/zmeu/runtime/foot_front.png",
    })
}

fn apply_player_appearance(parts: &mut [CutoutPart], appearance: PlayerAppearance) {
    let skin = skin_color(appearance.skin_tone);
    let garment = accent_color(appearance.accent);
    let cloth = limb_cloth_color(appearance.accent);
    let hair = hair_color(appearance.hair);

    for part in parts.iter_mut() {
        match part.kind {
            CutoutPartKind::Hair => {
                apply_hair_style(part, appearance.hair);
                part.color = hair;
            }
            CutoutPartKind::Torso => part.color = garment,
            CutoutPartKind::Head | CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
                part.color = skin;
            }
            CutoutPartKind::FootBack | CutoutPartKind::FootFront => {
                part.color = boot_color();
            }
            _ => part.color = cloth,
        }
        apply_build(part, appearance.build);
    }
}

fn apply_hair_style(part: &mut CutoutPart, hair: HairStyle) {
    match hair {
        HairStyle::Braided => {
            part.offset.x = -1.0;
            part.offset.y = 74.0;
            part.size.x = 28.0;
            part.size.y = 18.0;
        }
        HairStyle::Long => {
            part.offset.x = 0.0;
            part.offset.y = 70.0;
            part.size.x = 34.0;
            part.size.y = 25.0;
        }
        HairStyle::Short => {
            part.offset.x = 2.0;
            part.offset.y = 74.0;
            part.size.x = 26.0;
            part.size.y = 14.0;
        }
        HairStyle::Tied => {
            part.offset.x = 3.0;
            part.offset.y = 76.0;
            part.size.x = 24.0;
            part.size.y = 16.0;
        }
    }
}

fn apply_build(part: &mut CutoutPart, build: BodyBuild) {
    match build {
        BodyBuild::Lean => match part.kind {
            CutoutPartKind::Hair => {}
            CutoutPartKind::Torso => {
                part.size.x *= 0.92;
                part.size.y *= 0.96;
            }
            CutoutPartKind::Head => {
                part.size.x *= 0.96;
                part.size.y *= 0.96;
            }
            CutoutPartKind::UpperArmBack
            | CutoutPartKind::UpperArmFront
            | CutoutPartKind::ForearmBack
            | CutoutPartKind::ForearmFront
            | CutoutPartKind::ThighBack
            | CutoutPartKind::ThighFront
            | CutoutPartKind::ShinBack
            | CutoutPartKind::ShinFront => {
                part.size.x *= 0.9;
            }
            _ => {}
        },
        BodyBuild::Balanced => {}
        BodyBuild::Sturdy => match part.kind {
            CutoutPartKind::Hair => {}
            CutoutPartKind::Torso => {
                part.size.x *= 1.08;
                part.size.y *= 1.02;
            }
            CutoutPartKind::ThighBack
            | CutoutPartKind::ThighFront
            | CutoutPartKind::ShinBack
            | CutoutPartKind::ShinFront => {
                part.size.x *= 1.08;
            }
            _ => {}
        },
        BodyBuild::Powerful => match part.kind {
            CutoutPartKind::Hair => {}
            CutoutPartKind::Torso => {
                part.size.x *= 1.14;
                part.size.y *= 1.06;
                part.offset.y += 2.0;
            }
            CutoutPartKind::UpperArmBack
            | CutoutPartKind::UpperArmFront
            | CutoutPartKind::ForearmBack
            | CutoutPartKind::ForearmFront => {
                part.size.x *= 1.14;
                part.size.y *= 1.04;
            }
            CutoutPartKind::ThighBack
            | CutoutPartKind::ThighFront
            | CutoutPartKind::ShinBack
            | CutoutPartKind::ShinFront => {
                part.size.x *= 1.12;
                part.size.y *= 1.04;
            }
            CutoutPartKind::Head => {
                part.size.x *= 1.04;
                part.size.y *= 1.04;
            }
            _ => {}
        },
    }
}

fn skin_color(tone: SkinTone) -> Color {
    match tone {
        SkinTone::Fair => Color::srgb(0.92, 0.79, 0.66),
        SkinTone::Warm => Color::srgb(0.86, 0.68, 0.52),
        SkinTone::Olive => Color::srgb(0.71, 0.56, 0.39),
        SkinTone::Deep => Color::srgb(0.53, 0.36, 0.24),
    }
}

fn accent_color(accent: AccentColor) -> Color {
    match accent {
        AccentColor::Crimson => Color::srgb(0.72, 0.16, 0.16),
        AccentColor::Forest => Color::srgb(0.22, 0.41, 0.22),
        AccentColor::Gold => Color::srgb(0.73, 0.56, 0.18),
        AccentColor::Storm => Color::srgb(0.34, 0.38, 0.46),
    }
}

fn limb_cloth_color(accent: AccentColor) -> Color {
    match accent {
        AccentColor::Crimson => Color::srgb(0.9, 0.84, 0.72),
        AccentColor::Forest => Color::srgb(0.82, 0.87, 0.76),
        AccentColor::Gold => Color::srgb(0.91, 0.84, 0.61),
        AccentColor::Storm => Color::srgb(0.82, 0.82, 0.86),
    }
}

fn hair_color(hair: HairStyle) -> Color {
    match hair {
        HairStyle::Braided => Color::srgb(0.12, 0.08, 0.07),
        HairStyle::Long => Color::srgb(0.36, 0.22, 0.12),
        HairStyle::Short => Color::srgb(0.55, 0.38, 0.18),
        HairStyle::Tied => Color::srgb(0.48, 0.48, 0.5),
    }
}

fn boot_color() -> Color {
    Color::srgb(0.12, 0.08, 0.07)
}

fn human_color(kind: CutoutPartKind) -> Color {
    match kind {
        CutoutPartKind::Hair => hair_color(PlayerAppearance::default().hair),
        CutoutPartKind::Torso => Color::srgb(0.72, 0.16, 0.16),
        CutoutPartKind::Head | CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            Color::srgb(0.86, 0.68, 0.52)
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => Color::srgb(0.12, 0.08, 0.07),
        _ => Color::srgb(0.9, 0.84, 0.72),
    }
}

fn enemy_color(kind: CutoutPartKind) -> Color {
    match kind {
        CutoutPartKind::Hair => Color::srgb(0.18, 0.2, 0.22),
        CutoutPartKind::Torso => Color::srgb(0.24, 0.29, 0.34),
        CutoutPartKind::Head | CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            Color::srgb(0.62, 0.68, 0.72)
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => Color::srgb(0.08, 0.08, 0.09),
        _ => Color::srgb(0.46, 0.5, 0.54),
    }
}

fn boss_color(kind: CutoutPartKind) -> Color {
    match kind {
        CutoutPartKind::Hair => Color::srgb(0.18, 0.08, 0.07),
        CutoutPartKind::Torso => Color::srgb(0.52, 0.18, 0.1),
        CutoutPartKind::Head | CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            Color::srgb(0.8, 0.6, 0.42)
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => Color::srgb(0.1, 0.07, 0.06),
        _ => Color::srgb(0.62, 0.36, 0.2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Recursively collects every `CutoutPartMarker` entity under `root`, at
    /// any depth. Forearms/hands/shins/feet are nested several joints deep
    /// rather than being direct children of the root (#117), so callers
    /// that need "every body part this rig spawned" must walk the whole
    /// subtree instead of only `root`'s immediate `Children`.
    fn collect_rig_parts(world: &World, root: Entity) -> Vec<(CutoutPartKind, Entity)> {
        let mut found = Vec::new();
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            let Some(children) = world.get::<Children>(entity) else {
                continue;
            };
            for child in children.iter() {
                if let Some(marker) = world.get::<CutoutPartMarker>(child) {
                    found.push((marker.kind, child));
                }
                stack.push(child);
            }
        }
        found
    }

    fn count_children_with_parts(world: &mut World, root: Entity) -> usize {
        collect_rig_parts(world, root).len()
    }

    /// Finds `kind` among `root`'s direct [`Children`], regardless of
    /// whether that child in turn has body-part children of its own.
    fn direct_part_child(world: &World, root: Entity, kind: CutoutPartKind) -> Option<Entity> {
        world.get::<Children>(root)?.iter().find_map(|child| {
            world
                .get::<CutoutPartMarker>(child)
                .filter(|marker| marker.kind == kind)
                .map(|_| child)
        })
    }

    #[test]
    fn human_template_contains_the_required_body_parts() {
        let template = human_template();
        for kind in [
            CutoutPartKind::Torso,
            CutoutPartKind::Head,
            CutoutPartKind::UpperArmBack,
            CutoutPartKind::ForearmBack,
            CutoutPartKind::HandBack,
            CutoutPartKind::UpperArmFront,
            CutoutPartKind::ForearmFront,
            CutoutPartKind::HandFront,
            CutoutPartKind::ThighBack,
            CutoutPartKind::ShinBack,
            CutoutPartKind::FootBack,
            CutoutPartKind::ThighFront,
            CutoutPartKind::ShinFront,
            CutoutPartKind::FootFront,
        ] {
            assert!(
                template.parts.iter().any(|part| part.kind == kind),
                "{kind:?} exists"
            );
        }
        // Chained parts (forearms, hands, shins, feet) now store `z_offset`
        // relative to their parent part rather than the rig root (#117), so
        // the flat per-part field is no longer globally ascending -- check
        // the *composed* depth Bevy actually renders instead, via the same
        // parent-walk `effective_z_offset` uses.
        let draw_order = [
            CutoutPartKind::UpperArmBack,
            CutoutPartKind::ForearmBack,
            CutoutPartKind::HandBack,
            CutoutPartKind::ThighBack,
            CutoutPartKind::ShinBack,
            CutoutPartKind::FootBack,
            CutoutPartKind::Torso,
            CutoutPartKind::Hair,
            CutoutPartKind::Head,
            CutoutPartKind::UpperArmFront,
            CutoutPartKind::ForearmFront,
            CutoutPartKind::HandFront,
            CutoutPartKind::ThighFront,
            CutoutPartKind::ShinFront,
            CutoutPartKind::FootFront,
        ];
        assert!(
            draw_order
                .windows(2)
                .all(|pair| effective_z_offset(&template.parts, pair[0])
                    <= effective_z_offset(&template.parts, pair[1])),
            "parts render in the authored draw order once composed through the rig"
        );
    }

    #[test]
    fn representative_templates_change_layout_and_scale_not_only_color() {
        let human = human_template();
        let enemy = enemy_template();
        let boss = boss_template();
        assert_eq!(enemy.template, CutoutTemplate::Enemy);
        assert_eq!(boss.template, CutoutTemplate::Boss);

        let human_torso = human
            .parts
            .iter()
            .find(|part| part.kind == CutoutPartKind::Torso)
            .expect("human torso exists");
        let enemy_torso = enemy
            .parts
            .iter()
            .find(|part| part.kind == CutoutPartKind::Torso)
            .expect("enemy torso exists");
        let boss_torso = boss
            .parts
            .iter()
            .find(|part| part.kind == CutoutPartKind::Torso)
            .expect("boss torso exists");
        let human_head = human
            .parts
            .iter()
            .find(|part| part.kind == CutoutPartKind::Head)
            .expect("human head exists");
        let enemy_head = enemy
            .parts
            .iter()
            .find(|part| part.kind == CutoutPartKind::Head)
            .expect("enemy head exists");
        let boss_head = boss
            .parts
            .iter()
            .find(|part| part.kind == CutoutPartKind::Head)
            .expect("boss head exists");

        assert!(
            enemy_torso.size.x < human_torso.size.x,
            "enemy torso should read leaner than the human"
        );
        assert!(
            enemy_head.offset.y > human_head.offset.y,
            "enemy head should sit higher for a different silhouette"
        );
        assert!(
            boss_torso.size.x > human_torso.size.x,
            "boss torso should be broader than the human"
        );
        assert!(
            boss_head.size.y > human_head.size.y,
            "boss head should scale up with the larger body"
        );
    }

    #[test]
    fn spawning_a_rig_adds_one_marked_child_per_part() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        world.commands().queue(move |world: &mut World| {
            let mut commands = world.commands();
            spawn_cutout_rig(&mut commands, root, human_template(), None, false);
        });
        world.flush();

        let rig = world.get::<CutoutRig>(root).expect("root has rig marker");
        assert_eq!(rig.template, CutoutTemplate::Human);
        assert_eq!(
            count_children_with_parts(&mut world, root),
            human_template().parts.len()
        );
    }

    #[test]
    fn forearms_and_hands_nest_under_their_own_joint_not_the_rig_root() {
        // #117: forearms/hands/shins/feet used to be spawned as independent
        // children of the rig root with absolute body-space offsets, so a
        // forearm's offset never followed its upper arm's rotation and any
        // non-zero rotation opened a gap at the joint. Reparenting through
        // Bevy's transform hierarchy (which already propagates rotation to
        // children) is the fix -- assert the hierarchy actually nests that
        // way, for both the "back" and "front" arm/leg pairs.
        let mut world = World::new();
        let root = world.spawn_empty().id();
        world.commands().queue(move |world: &mut World| {
            let mut commands = world.commands();
            spawn_cutout_rig(&mut commands, root, human_template(), None, false);
        });
        world.flush();

        for (upper_arm_kind, forearm_kind, hand_kind) in [
            (
                CutoutPartKind::UpperArmBack,
                CutoutPartKind::ForearmBack,
                CutoutPartKind::HandBack,
            ),
            (
                CutoutPartKind::UpperArmFront,
                CutoutPartKind::ForearmFront,
                CutoutPartKind::HandFront,
            ),
        ] {
            let upper_arm = direct_part_child(&world, root, upper_arm_kind)
                .unwrap_or_else(|| panic!("{upper_arm_kind:?} is a direct child of the rig root"));
            assert!(
                direct_part_child(&world, root, forearm_kind).is_none(),
                "{forearm_kind:?} must not be a direct child of the rig root"
            );
            let forearm = direct_part_child(&world, upper_arm, forearm_kind).unwrap_or_else(|| {
                panic!("{forearm_kind:?} is a direct child of its {upper_arm_kind:?}")
            });
            assert!(
                direct_part_child(&world, root, hand_kind).is_none(),
                "{hand_kind:?} must not be a direct child of the rig root"
            );
            let _hand = direct_part_child(&world, forearm, hand_kind).unwrap_or_else(|| {
                panic!("{hand_kind:?} is a direct child of its {forearm_kind:?}")
            });
        }

        for (thigh_kind, shin_kind, foot_kind) in [
            (
                CutoutPartKind::ThighBack,
                CutoutPartKind::ShinBack,
                CutoutPartKind::FootBack,
            ),
            (
                CutoutPartKind::ThighFront,
                CutoutPartKind::ShinFront,
                CutoutPartKind::FootFront,
            ),
        ] {
            let thigh = direct_part_child(&world, root, thigh_kind)
                .unwrap_or_else(|| panic!("{thigh_kind:?} is a direct child of the rig root"));
            assert!(
                direct_part_child(&world, root, shin_kind).is_none(),
                "{shin_kind:?} must not be a direct child of the rig root"
            );
            let shin = direct_part_child(&world, thigh, shin_kind)
                .unwrap_or_else(|| panic!("{shin_kind:?} is a direct child of its {thigh_kind:?}"));
            assert!(
                direct_part_child(&world, root, foot_kind).is_none(),
                "{foot_kind:?} must not be a direct child of the rig root"
            );
            let _foot = direct_part_child(&world, shin, foot_kind)
                .unwrap_or_else(|| panic!("{foot_kind:?} is a direct child of its {shin_kind:?}"));
        }
    }

    #[test]
    fn rotating_the_upper_arm_keeps_the_forearm_at_its_elbow_attachment_point() {
        // #117: previously the forearm's position was independent of the
        // upper arm's rotation (both were unlinked siblings of the root),
        // so any non-zero rotation opened a gap. With the forearm nested as
        // the upper arm's Bevy child, composing the two transforms should
        // always land the forearm exactly at the upper arm's documented
        // elbow `pivot`, however the upper arm is rotated (e.g. mid-punch).
        let template = human_template();
        let upper_arm_part = template
            .parts
            .iter()
            .find(|part| part.kind == CutoutPartKind::UpperArmFront)
            .expect("upper arm front exists");
        let elbow_pivot = upper_arm_part.pivot;
        assert_ne!(
            elbow_pivot,
            Vec2::ZERO,
            "the upper arm documents an elbow pivot"
        );

        let mut world = World::new();
        let root = world.spawn_empty().id();
        world.commands().queue(move |world: &mut World| {
            let mut commands = world.commands();
            spawn_cutout_rig(&mut commands, root, human_template(), None, false);
        });
        world.flush();

        let upper_arm = direct_part_child(&world, root, CutoutPartKind::UpperArmFront)
            .expect("upper arm front is a direct child of the root");
        let forearm = direct_part_child(&world, upper_arm, CutoutPartKind::ForearmFront)
            .expect("forearm front is a direct child of the upper arm");

        // Simulate an arbitrary combat pose rotating the upper arm in place
        // (this is exactly what `arena::animation::apply_cutout_poses` does
        // for Attack/Block/Dodge/etc every frame).
        world
            .get_mut::<Transform>(upper_arm)
            .expect("upper arm has a transform")
            .rotate_z(0.6);

        let upper_arm_transform = *world.get::<Transform>(upper_arm).unwrap();
        let forearm_transform = *world.get::<Transform>(forearm).unwrap();
        let upper_arm_global = GlobalTransform::from(upper_arm_transform);

        let expected_elbow = upper_arm_global.transform_point(elbow_pivot.extend(0.0));
        let forearm_global = upper_arm_global.mul_transform(forearm_transform);

        let epsilon = 1e-3;
        assert!(
            (forearm_global.translation().truncate() - expected_elbow.truncate()).length()
                < epsilon,
            "forearm should stay glued to the rotated elbow attachment point: \
             forearm at {:?}, expected near {:?}",
            forearm_global.translation(),
            expected_elbow
        );
    }

    #[test]
    fn mirrored_rigs_invert_part_x_offsets() {
        let mut world = World::new();
        let normal = world.spawn_empty().id();
        let mirrored = world.spawn_empty().id();
        world.commands().queue(move |world: &mut World| {
            let mut commands = world.commands();
            spawn_cutout_rig(&mut commands, normal, human_template(), None, false);
            spawn_cutout_rig(&mut commands, mirrored, human_template(), None, true);
        });
        world.flush();

        let normal_parts = collect_rig_parts(&world, normal);
        let mirrored_parts = collect_rig_parts(&world, mirrored);
        assert_eq!(normal_parts.len(), human_template().parts.len());
        assert_eq!(mirrored_parts.len(), human_template().parts.len());

        for (kind, left) in normal_parts {
            let (_, right) = mirrored_parts
                .iter()
                .find(|(mirrored_kind, _)| *mirrored_kind == kind)
                .unwrap_or_else(|| panic!("mirrored rig also has a {kind:?}"));
            let left_transform = world.get::<Transform>(left).unwrap();
            let right_transform = world.get::<Transform>(*right).unwrap();
            assert_eq!(
                left_transform.translation.x, -right_transform.translation.x,
                "{kind:?} x offset mirrors"
            );
            assert_eq!(
                left_transform.translation.y, right_transform.translation.y,
                "{kind:?} y offset is unchanged by mirroring"
            );
            assert_eq!(
                left_transform.translation.z, right_transform.translation.z,
                "{kind:?} z offset is unchanged by mirroring"
            );
        }
    }

    #[test]
    fn spawned_body_part_sprites_flip_with_the_rig() {
        let mut world = World::new();
        let player = world.spawn_empty().id();
        let enemy = world.spawn_empty().id();
        world.commands().queue(move |world: &mut World| {
            let mut commands = world.commands();
            spawn_cutout_rig(&mut commands, player, human_template(), None, false);
            spawn_cutout_rig(&mut commands, enemy, enemy_template(), None, true);
        });
        world.flush();

        let player_parts = collect_rig_parts(&world, player);
        assert!(!player_parts.is_empty(), "player rig has body parts");
        for (_, child) in player_parts {
            let sprite = world.get::<Sprite>(child).expect("body part has a sprite");
            assert!(
                !sprite.flip_x,
                "player rig sprites stay unflipped (byte-for-byte unchanged)"
            );
        }

        let enemy_parts = collect_rig_parts(&world, enemy);
        assert!(!enemy_parts.is_empty(), "enemy rig has body parts");
        for (_, child) in enemy_parts {
            let sprite = world.get::<Sprite>(child).expect("body part has a sprite");
            assert!(
                sprite.flip_x,
                "enemy rig sprites mirror the artwork so the fighter faces the player"
            );
        }
    }

    #[test]
    fn gear_layer_sprites_spawned_through_the_gear_rig_helper_stay_unflipped() {
        // spawn_cutout_rig_with_gear is only ever used by player-facing preview
        // screens (shop/creation) with flip_x = false; assert the gear-layer
        // sprites it produces stay unflipped so that path remains
        // byte-for-byte unchanged by this fix.
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let mut equipment = Equipment::default();
        equipment.equip(ItemId::Palos);
        world.commands().queue(move |world: &mut World| {
            let mut commands = world.commands();
            spawn_cutout_rig_with_gear(
                &mut commands,
                root,
                human_template(),
                None,
                false,
                &equipment,
            );
        });
        world.flush();

        let mut found_gear_layer = false;
        let mut query = world.query::<(&GearVisualLayer, &Sprite)>();
        for (_, sprite) in query.iter(&world) {
            found_gear_layer = true;
            assert!(!sprite.flip_x, "player gear layers stay unflipped");
        }
        assert!(found_gear_layer, "the equipped weapon spawns a gear layer");
    }

    #[test]
    fn playable_cutout_templates_use_runtime_body_part_assets() {
        let cases = [
            (
                "human",
                human_template(),
                &[
                    (CutoutPartKind::Hair, "fighters/human/runtime/hair.png"),
                    (CutoutPartKind::Head, "fighters/human/runtime/head.png"),
                    (CutoutPartKind::Torso, "fighters/human/runtime/torso.png"),
                    (
                        CutoutPartKind::UpperArmBack,
                        "fighters/human/runtime/upper_arm_back.png",
                    ),
                    (
                        CutoutPartKind::UpperArmFront,
                        "fighters/human/runtime/upper_arm_front.png",
                    ),
                    (
                        CutoutPartKind::ForearmBack,
                        "fighters/human/runtime/forearm_back.png",
                    ),
                    (
                        CutoutPartKind::ForearmFront,
                        "fighters/human/runtime/forearm_front.png",
                    ),
                    (
                        CutoutPartKind::HandBack,
                        "fighters/human/runtime/hand_back.png",
                    ),
                    (
                        CutoutPartKind::HandFront,
                        "fighters/human/runtime/hand_front.png",
                    ),
                    (
                        CutoutPartKind::ThighBack,
                        "fighters/human/runtime/thigh_back.png",
                    ),
                    (
                        CutoutPartKind::ThighFront,
                        "fighters/human/runtime/thigh_front.png",
                    ),
                    (
                        CutoutPartKind::ShinBack,
                        "fighters/human/runtime/shin_back.png",
                    ),
                    (
                        CutoutPartKind::ShinFront,
                        "fighters/human/runtime/shin_front.png",
                    ),
                    (
                        CutoutPartKind::FootBack,
                        "fighters/human/runtime/foot_back.png",
                    ),
                    (
                        CutoutPartKind::FootFront,
                        "fighters/human/runtime/foot_front.png",
                    ),
                ][..],
            ),
            (
                "strigoi",
                enemy_template(),
                &[
                    (CutoutPartKind::Head, "fighters/strigoi/runtime/head.png"),
                    (CutoutPartKind::Torso, "fighters/strigoi/runtime/torso.png"),
                    (
                        CutoutPartKind::UpperArmBack,
                        "fighters/strigoi/runtime/upper_arm_back.png",
                    ),
                    (
                        CutoutPartKind::UpperArmFront,
                        "fighters/strigoi/runtime/upper_arm_front.png",
                    ),
                    (
                        CutoutPartKind::ForearmBack,
                        "fighters/strigoi/runtime/forearm_back.png",
                    ),
                    (
                        CutoutPartKind::ForearmFront,
                        "fighters/strigoi/runtime/forearm_front.png",
                    ),
                    (
                        CutoutPartKind::HandBack,
                        "fighters/strigoi/runtime/hand_back.png",
                    ),
                    (
                        CutoutPartKind::HandFront,
                        "fighters/strigoi/runtime/hand_front.png",
                    ),
                    (
                        CutoutPartKind::ThighBack,
                        "fighters/strigoi/runtime/thigh_back.png",
                    ),
                    (
                        CutoutPartKind::ThighFront,
                        "fighters/strigoi/runtime/thigh_front.png",
                    ),
                    (
                        CutoutPartKind::ShinBack,
                        "fighters/strigoi/runtime/shin_back.png",
                    ),
                    (
                        CutoutPartKind::ShinFront,
                        "fighters/strigoi/runtime/shin_front.png",
                    ),
                    (
                        CutoutPartKind::FootBack,
                        "fighters/strigoi/runtime/foot_back.png",
                    ),
                    (
                        CutoutPartKind::FootFront,
                        "fighters/strigoi/runtime/foot_front.png",
                    ),
                ][..],
            ),
            (
                "zmeu",
                boss_template(),
                &[
                    (CutoutPartKind::Head, "fighters/zmeu/runtime/head.png"),
                    (CutoutPartKind::Torso, "fighters/zmeu/runtime/torso.png"),
                    (
                        CutoutPartKind::UpperArmBack,
                        "fighters/zmeu/runtime/upper_arm_back.png",
                    ),
                    (
                        CutoutPartKind::UpperArmFront,
                        "fighters/zmeu/runtime/upper_arm_front.png",
                    ),
                    (
                        CutoutPartKind::ForearmBack,
                        "fighters/zmeu/runtime/forearm_back.png",
                    ),
                    (
                        CutoutPartKind::ForearmFront,
                        "fighters/zmeu/runtime/forearm_front.png",
                    ),
                    (
                        CutoutPartKind::HandBack,
                        "fighters/zmeu/runtime/hand_back.png",
                    ),
                    (
                        CutoutPartKind::HandFront,
                        "fighters/zmeu/runtime/hand_front.png",
                    ),
                    (
                        CutoutPartKind::ThighBack,
                        "fighters/zmeu/runtime/thigh_back.png",
                    ),
                    (
                        CutoutPartKind::ThighFront,
                        "fighters/zmeu/runtime/thigh_front.png",
                    ),
                    (
                        CutoutPartKind::ShinBack,
                        "fighters/zmeu/runtime/shin_back.png",
                    ),
                    (
                        CutoutPartKind::ShinFront,
                        "fighters/zmeu/runtime/shin_front.png",
                    ),
                    (
                        CutoutPartKind::FootBack,
                        "fighters/zmeu/runtime/foot_back.png",
                    ),
                    (
                        CutoutPartKind::FootFront,
                        "fighters/zmeu/runtime/foot_front.png",
                    ),
                ][..],
            ),
        ];
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");

        for (label, template, expected_paths) in cases {
            for (kind, expected_path) in expected_paths {
                let part = template
                    .parts
                    .iter()
                    .find(|part| part.kind == *kind)
                    .unwrap_or_else(|| panic!("{label} template missing {kind:?}"));
                assert_eq!(part.asset_path, Some(*expected_path), "{label} {kind:?}");
                assert!(
                    assets.join(expected_path).is_file(),
                    "{label} {kind:?} asset missing at {}",
                    assets.join(expected_path).display()
                );
            }
        }
    }
}
