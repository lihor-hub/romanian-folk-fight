//! Runtime cutout-rig rendering: small, explicit body-part templates that can
//! be attached to creator previews and arena fighter roots. This slice wires
//! the first production-intent pixel-art body parts and starter gear into the
//! runtime while keeping the ECS surface compact.

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
    pub offset: Vec2,
    pub size: Vec2,
    pub rotation: f32,
    pub z_offset: f32,
    pub color: Color,
    pub asset_path: Option<&'static str>,
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

/// First enemy neutral pose. It reuses the same part categories with leaner,
/// slightly taller proportions to prove non-player use of the path.
pub fn enemy_template() -> CutoutRigTemplate {
    CutoutRigTemplate {
        template: CutoutTemplate::Enemy,
        parts: human_parts(1.0).into_iter().map(strigoi_part).collect(),
    }
}

/// Larger boss-style neutral pose with broader proportions and a taller
/// silhouette so major enemies read differently at a glance.
pub fn boss_template() -> CutoutRigTemplate {
    CutoutRigTemplate {
        template: CutoutTemplate::Boss,
        parts: human_parts(1.26).into_iter().map(zmeu_part).collect(),
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
    commands.entity(root).insert(CutoutRig {
        template: template.template,
        flip_x,
    });
    commands.entity(root).insert(CutoutPose::Idle);
    commands.entity(root).with_children(|body| {
        for part in template.parts {
            let transform = part_transform(&part, flip_x);
            body.spawn((
                CutoutPartMarker { kind: part.kind },
                CutoutPartRestPose {
                    transform,
                    size: part.size,
                },
                part_sprite(&part, asset_server),
                transform,
            ));
        }
    });
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
    commands.entity(root).insert(CutoutRig {
        template: template.template,
        flip_x,
    });
    commands.entity(root).insert(CutoutPose::Idle);
    commands.entity(root).with_children(|body| {
        for part in template.parts {
            let transform = part_transform(&part, flip_x);
            body.spawn((
                CutoutPartMarker { kind: part.kind },
                CutoutPartRestPose {
                    transform,
                    size: part.size,
                },
                part_sprite(&part, asset_server),
                transform,
            ))
            .with_children(|part_children| {
                spawn_gear_children_for_part(
                    part_children,
                    part.kind,
                    equipment,
                    asset_server,
                    &mut |_| (),
                );
            });
        }
    });
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

fn part_sprite(part: &CutoutPart, asset_server: Option<&AssetServer>) -> Sprite {
    match (asset_server, part.asset_path) {
        (Some(asset_server), Some(path)) => Sprite {
            custom_size: Some(part.size),
            ..Sprite::from_image(asset_server.load(path))
        },
        _ => Sprite::from_color(part.color, part.size),
    }
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

fn human_parts(scale: f32) -> Vec<CutoutPart> {
    vec![
        part(
            CutoutPartKind::UpperArmBack,
            -20.0,
            26.0,
            15.0,
            44.0,
            -0.18,
            -0.08,
        ),
        part(
            CutoutPartKind::ForearmBack,
            -28.0,
            -2.0,
            13.0,
            38.0,
            -0.28,
            -0.07,
        ),
        part(
            CutoutPartKind::HandBack,
            -32.0,
            -26.0,
            13.0,
            13.0,
            -0.1,
            -0.06,
        ),
        part(
            CutoutPartKind::ThighBack,
            -13.0,
            -42.0,
            17.0,
            42.0,
            0.08,
            -0.05,
        ),
        part(
            CutoutPartKind::ShinBack,
            -15.0,
            -76.0,
            14.0,
            38.0,
            -0.05,
            -0.04,
        ),
        part(
            CutoutPartKind::FootBack,
            -8.0,
            -102.0,
            28.0,
            12.0,
            0.0,
            -0.03,
        ),
        part(CutoutPartKind::Torso, 0.0, 6.0, 44.0, 74.0, 0.0, 0.0),
        part(CutoutPartKind::Hair, 1.0, 71.0, 32.0, 20.0, 0.02, 0.02),
        part(CutoutPartKind::Head, 4.0, 60.0, 38.0, 42.0, 0.04, 0.03),
        part(
            CutoutPartKind::UpperArmFront,
            21.0,
            25.0,
            15.0,
            45.0,
            -0.12,
            0.04,
        ),
        part(
            CutoutPartKind::ForearmFront,
            29.0,
            -3.0,
            13.0,
            39.0,
            -0.18,
            0.05,
        ),
        part(
            CutoutPartKind::HandFront,
            33.0,
            -28.0,
            13.0,
            13.0,
            -0.04,
            0.06,
        ),
        part(
            CutoutPartKind::ThighFront,
            13.0,
            -42.0,
            17.0,
            42.0,
            -0.07,
            0.07,
        ),
        part(
            CutoutPartKind::ShinFront,
            15.0,
            -76.0,
            14.0,
            38.0,
            0.04,
            0.08,
        ),
        part(
            CutoutPartKind::FootFront,
            23.0,
            -102.0,
            28.0,
            12.0,
            0.0,
            0.09,
        ),
    ]
    .into_iter()
    .map(|mut part| {
        part.offset *= scale;
        part.size *= scale;
        part.asset_path = human_asset_path(part.kind);
        part
    })
    .collect()
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
        }
        CutoutPartKind::Torso => {
            part.offset.y += 6.0;
            part.size.x *= 0.72;
            part.size.y *= 0.94;
        }
        CutoutPartKind::Head => {
            part.offset.x -= 4.0;
            part.offset.y += 12.0;
            part.size.x *= 1.18;
            part.size.y *= 1.22;
        }
        CutoutPartKind::UpperArmBack | CutoutPartKind::UpperArmFront => {
            part.offset.x *= 0.82;
            part.offset.y += 8.0;
            part.size.x *= 0.86;
            part.size.y *= 1.18;
        }
        CutoutPartKind::ForearmBack | CutoutPartKind::ForearmFront => {
            part.offset.x *= 0.8;
            part.offset.y += 4.0;
            part.size.x *= 0.84;
            part.size.y *= 1.22;
        }
        CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            part.offset.x *= 0.8;
            part.offset.y += 2.0;
            part.size.x *= 0.9;
            part.size.y *= 1.08;
        }
        CutoutPartKind::ThighBack | CutoutPartKind::ThighFront => {
            part.offset.x *= 0.92;
            part.offset.y -= 2.0;
            part.size.x *= 0.9;
            part.size.y *= 1.08;
        }
        CutoutPartKind::ShinBack | CutoutPartKind::ShinFront => {
            part.offset.x *= 0.9;
            part.offset.y -= 6.0;
            part.size.x *= 0.86;
            part.size.y *= 1.16;
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => {
            part.offset.x *= 0.82;
            part.offset.y -= 8.0;
            part.size.x *= 0.76;
            part.size.y *= 0.9;
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
        }
        CutoutPartKind::Torso => {
            part.offset.y += 10.0;
            part.size.x *= 1.34;
            part.size.y *= 1.2;
        }
        CutoutPartKind::Head => {
            part.offset.x += 4.0;
            part.offset.y += 18.0;
            part.size.x *= 1.14;
            part.size.y *= 1.18;
        }
        CutoutPartKind::UpperArmBack | CutoutPartKind::UpperArmFront => {
            part.offset.x *= 1.16;
            part.offset.y += 8.0;
            part.size.x *= 1.42;
            part.size.y *= 1.12;
        }
        CutoutPartKind::ForearmBack | CutoutPartKind::ForearmFront => {
            part.offset.x *= 1.16;
            part.offset.y += 4.0;
            part.size.x *= 1.34;
            part.size.y *= 1.14;
        }
        CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            part.offset.x *= 1.18;
            part.size.x *= 1.28;
            part.size.y *= 1.2;
        }
        CutoutPartKind::ThighBack | CutoutPartKind::ThighFront => {
            part.offset.x *= 1.08;
            part.offset.y -= 4.0;
            part.size.x *= 1.34;
            part.size.y *= 1.12;
        }
        CutoutPartKind::ShinBack | CutoutPartKind::ShinFront => {
            part.offset.x *= 1.08;
            part.offset.y -= 10.0;
            part.size.x *= 1.28;
            part.size.y *= 1.1;
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => {
            part.offset.x *= 1.12;
            part.offset.y -= 12.0;
            part.size.x *= 1.38;
            part.size.y *= 1.22;
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

/// Resolved sprite colors for one saved [`PlayerAppearance`]. Exposed so
/// creator preview tests can assert per-preset preview rig colors without
/// duplicating the palette lookup tables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerAppearancePalette {
    pub skin: Color,
    pub garment: Color,
    pub cloth: Color,
    pub hair: Color,
    pub boot: Color,
}

/// Returns the sprite palette [`human_template_for`] paints onto each body
/// part for `appearance`.
pub fn player_appearance_palette(appearance: PlayerAppearance) -> PlayerAppearancePalette {
    PlayerAppearancePalette {
        skin: skin_color(appearance.skin_tone),
        garment: accent_color(appearance.accent),
        cloth: limb_cloth_color(appearance.accent),
        hair: hair_color(appearance.hair),
        boot: boot_color(),
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

    fn count_children_with_parts(world: &mut World, root: Entity) -> usize {
        let children = world
            .get::<Children>(root)
            .expect("rig root has body-part children")
            .to_vec();
        children
            .into_iter()
            .filter(|child| world.get::<CutoutPartMarker>(*child).is_some())
            .count()
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
        assert!(
            template
                .parts
                .windows(2)
                .all(|pair| pair[0].z_offset <= pair[1].z_offset),
            "parts are authored in draw order"
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

        let normal_children = world.get::<Children>(normal).unwrap().to_vec();
        let mirrored_children = world.get::<Children>(mirrored).unwrap().to_vec();
        for (left, right) in normal_children.iter().zip(mirrored_children.iter()) {
            let left_transform = world.get::<Transform>(*left).unwrap();
            let right_transform = world.get::<Transform>(*right).unwrap();
            assert_eq!(left_transform.translation.x, -right_transform.translation.x);
            assert_eq!(left_transform.translation.y, right_transform.translation.y);
            assert_eq!(left_transform.translation.z, right_transform.translation.z);
        }
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
