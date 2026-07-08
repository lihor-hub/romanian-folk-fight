//! Runtime cutout-rig rendering: small, explicit body-part templates that can
//! be attached to creator previews and arena fighter roots. The first slice
//! uses placeholder sprites so ECS wiring is testable before production source
//! sheets are sliced into final transparent PNG parts.

use bevy::prelude::*;

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
}

/// Stable body-part identifiers. Front/back variants give the first rig a
/// usable draw order while staying close to the planned anatomical part set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CutoutPartKind {
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

/// Human/player neutral pose.
pub fn human_template() -> CutoutRigTemplate {
    CutoutRigTemplate {
        template: CutoutTemplate::Human,
        parts: human_parts(1.0),
    }
}

/// First enemy neutral pose. It reuses the same part categories with leaner,
/// slightly taller proportions to prove non-player use of the path.
pub fn enemy_template() -> CutoutRigTemplate {
    CutoutRigTemplate {
        template: CutoutTemplate::Enemy,
        parts: human_parts(1.08)
            .into_iter()
            .map(|mut part| {
                part.offset.x *= 0.9;
                part.offset.y *= 1.04;
                part.size.x *= 0.82;
                part.size.y *= 1.08;
                part.color = enemy_color(part.kind);
                part
            })
            .collect(),
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
    commands.entity(root).with_children(|body| {
        for part in template.parts {
            body.spawn((
                CutoutPartMarker { kind: part.kind },
                part_sprite(&part, asset_server),
                part_transform(&part, flip_x),
            ));
        }
    });
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
        part
    })
    .collect()
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

fn human_color(kind: CutoutPartKind) -> Color {
    match kind {
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
        CutoutPartKind::Torso => Color::srgb(0.24, 0.29, 0.34),
        CutoutPartKind::Head | CutoutPartKind::HandBack | CutoutPartKind::HandFront => {
            Color::srgb(0.62, 0.68, 0.72)
        }
        CutoutPartKind::FootBack | CutoutPartKind::FootFront => Color::srgb(0.08, 0.08, 0.09),
        _ => Color::srgb(0.46, 0.5, 0.54),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
