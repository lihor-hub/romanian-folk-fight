//! Resolved character-material data and the hybrid 2D rendering path.

use bevy::{
    image::ImageLoaderSettings,
    math::primitives::Rectangle,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d},
};

use super::{PaletteRegion, PartId, PartLayerRecord};

const HYBRID_CHARACTER_SHADER: &str = "shaders/hybrid_character_2d.wgsl";

/// Renderer-owned contact-shadow settings derived from authored metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedShadowSettings {
    pub asset_path: String,
    pub strength: f32,
}

/// Material inputs resolved from one stable catalog record.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPartMaterial {
    pub part_id: PartId,
    pub albedo_path: String,
    pub mask_path: Option<String>,
    pub normal_path: Option<String>,
    pub shadow: Option<ResolvedShadowSettings>,
    pub palette: Vec<PaletteRegion>,
    pub depth_offset: f32,
    pub highlight: f32,
}

#[derive(Debug, Clone, Copy, ShaderType)]
pub struct HybridCharacterUniforms {
    tint: Vec4,
    palette_0: Vec4,
    palette_1: Vec4,
    palette_2: Vec4,
    /// x: depth, y: highlight, z: shadow strength, w: palette count.
    settings: Vec4,
    /// x: horizontal UV mirror flag.
    render_flags: Vec4,
}

/// GPU material used only after every authored channel has loaded.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct HybridCharacterMaterial {
    #[uniform(0)]
    uniforms: HybridCharacterUniforms,
    #[texture(1)]
    #[sampler(2)]
    albedo: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    mask: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    normal: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    shadow: Handle<Image>,
}

impl Material2d for HybridCharacterMaterial {
    fn fragment_shader() -> ShaderRef {
        HYBRID_CHARACTER_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Mask(0.5)
    }
}

/// Complete handle set retained beside the fallback sprite while assets load.
#[derive(Component, Debug, Clone)]
pub(crate) struct PendingHybridCharacterMaterial {
    pub resolved: ResolvedPartMaterial,
    pub albedo: Handle<Image>,
    pub mask: Handle<Image>,
    pub normal: Handle<Image>,
    pub shadow: Handle<Image>,
    pub size: Vec2,
    pub color: Color,
    pub flip_x: bool,
}

pub(crate) fn pending_hybrid_material_for(
    resolved: &ResolvedPartMaterial,
    albedo: Handle<Image>,
    asset_server: &AssetServer,
    size: Vec2,
    color: Color,
    flip_x: bool,
) -> Option<PendingHybridCharacterMaterial> {
    let mask_path = resolved.mask_path.as_ref()?;
    let normal_path = resolved.normal_path.as_ref()?;
    let shadow_path = &resolved.shadow.as_ref()?.asset_path;
    Some(PendingHybridCharacterMaterial {
        resolved: resolved.clone(),
        albedo,
        mask: load_linear_image(asset_server, mask_path),
        normal: load_linear_image(asset_server, normal_path),
        shadow: load_linear_image(asset_server, shadow_path),
        size,
        color,
        flip_x,
    })
}

fn load_linear_image(asset_server: &AssetServer, path: &str) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| settings.is_srgb = false)
        .load(path.to_owned())
}

/// Promotes complete, loaded channel sets from the safe sprite path.
pub(crate) fn promote_ready_hybrid_materials(
    mut commands: Commands,
    images: Option<Res<Assets<Image>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<HybridCharacterMaterial>>>,
    pending_parts: Query<(Entity, &PendingHybridCharacterMaterial)>,
) {
    let (Some(images), Some(mut meshes), Some(mut materials)) = (images, meshes, materials) else {
        return;
    };
    for (entity, pending) in &pending_parts {
        if ![
            &pending.albedo,
            &pending.mask,
            &pending.normal,
            &pending.shadow,
        ]
        .into_iter()
        .all(|handle| images.contains(handle.id()))
        {
            continue;
        }

        let palette = resolved_palette(&pending.resolved.palette, pending.color);
        let material = HybridCharacterMaterial {
            uniforms: HybridCharacterUniforms {
                tint: color_vec4(pending.color),
                palette_0: palette[0],
                palette_1: palette[1],
                palette_2: palette[2],
                settings: Vec4::new(
                    pending.resolved.depth_offset,
                    pending.resolved.highlight,
                    pending
                        .resolved
                        .shadow
                        .as_ref()
                        .map_or(0.0, |shadow| shadow.strength),
                    pending.resolved.palette.len().min(3) as f32,
                ),
                render_flags: Vec4::new(if pending.flip_x { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0),
            },
            albedo: pending.albedo.clone(),
            mask: pending.mask.clone(),
            normal: pending.normal.clone(),
            shadow: pending.shadow.clone(),
        };

        commands
            .entity(entity)
            .remove::<Sprite>()
            .remove::<PendingHybridCharacterMaterial>()
            .insert((
                Mesh2d(meshes.add(Rectangle::from_size(pending.size))),
                MeshMaterial2d(materials.add(material)),
            ));
    }
}

fn resolved_palette(regions: &[PaletteRegion], base_color: Color) -> [Vec4; 3] {
    let mut colors = [color_vec4(base_color); 3];
    for (target, region) in colors.iter_mut().zip(regions.iter().take(3)) {
        *target = match region {
            PaletteRegion::Skin | PaletteRegion::Hair | PaletteRegion::Cloth => {
                color_vec4(base_color)
            }
            PaletteRegion::Embroidery => color_vec4(Color::srgb(0.79, 0.64, 0.15)),
            PaletteRegion::Leather => color_vec4(Color::srgb(0.36, 0.18, 0.09)),
            PaletteRegion::Metal => color_vec4(Color::srgb(0.58, 0.62, 0.66)),
        };
    }
    colors
}

fn color_vec4(color: Color) -> Vec4 {
    let linear = color.to_linear();
    Vec4::new(linear.red, linear.green, linear.blue, linear.alpha)
}

/// Resolves renderer inputs without moving asset-path ownership out of the
/// validated catalog layer or losing its semantic stable-ID provenance.
pub fn resolve_material_for_layer(
    part_id: &PartId,
    layer: &PartLayerRecord,
) -> ResolvedPartMaterial {
    let depth_offset = bounded_or_default(layer.material.depth_offset, 0.0, -1.0, 1.0);
    let highlight = bounded_or_default(layer.material.highlight, 0.0, 0.0, 1.0);
    let shadow = layer
        .material
        .shadow_path
        .as_ref()
        .map(|asset_path| ResolvedShadowSettings {
            asset_path: asset_path.clone(),
            // Authored depth can make contact slightly more legible, but the
            // renderer owns a deliberately restrained upper bound.
            strength: (0.18 + depth_offset.abs() * 0.12).min(0.35),
        });

    ResolvedPartMaterial {
        part_id: part_id.clone(),
        albedo_path: layer.asset_path.clone(),
        mask_path: layer.material.mask_path.clone(),
        normal_path: layer.material.normal_path.clone(),
        shadow,
        palette: layer.material.palette.clone(),
        depth_offset,
        highlight,
    }
}

fn bounded_or_default(value: Option<f32>, default: f32, min: f32, max: f32) -> f32 {
    match value {
        Some(value) if value.is_finite() => value.clamp(min, max),
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{AttachmentMetadata, MaterialMetadata};

    fn layer_with(material: MaterialMetadata) -> (PartId, PartLayerRecord) {
        (
            PartId::new("human.torso.material-test.v1").expect("test ID is valid"),
            PartLayerRecord {
                asset_path: "fighters/human/runtime/torso.png".to_owned(),
                attachment: AttachmentMetadata {
                    point: "torso".to_owned(),
                    pivot: [0.5, 0.5],
                    draw_layer: 0,
                },
                material,
            },
        )
    }

    #[test]
    fn complete_material_resolves_from_the_stable_part_record() {
        let (part_id, layer) = layer_with(MaterialMetadata {
            mask_path: Some("fighters/human/runtime/torso_mask.png".to_owned()),
            normal_path: Some("fighters/human/runtime/torso_normal.png".to_owned()),
            shadow_path: Some("fighters/human/runtime/torso_shadow.png".to_owned()),
            palette: vec![PaletteRegion::Cloth, PaletteRegion::Embroidery],
            depth_offset: Some(0.4),
            highlight: Some(0.65),
        });

        let resolved = resolve_material_for_layer(&part_id, &layer);

        assert_eq!(resolved.part_id, part_id);
        assert_eq!(resolved.albedo_path, layer.asset_path);
        assert_eq!(
            resolved.mask_path.as_deref(),
            Some("fighters/human/runtime/torso_mask.png")
        );
        assert_eq!(
            resolved.normal_path.as_deref(),
            Some("fighters/human/runtime/torso_normal.png")
        );
        let shadow = resolved.shadow.expect("authored shadow is resolved");
        assert_eq!(shadow.asset_path, "fighters/human/runtime/torso_shadow.png");
        assert!((0.0..=0.35).contains(&shadow.strength));
        assert_eq!(
            resolved.palette,
            vec![PaletteRegion::Cloth, PaletteRegion::Embroidery]
        );
        assert_eq!(resolved.depth_offset, 0.4);
        assert_eq!(resolved.highlight, 0.65);
    }

    #[test]
    fn absent_optional_channels_resolve_to_none_and_neutral_settings() {
        let (part_id, layer) = layer_with(MaterialMetadata::default());

        let resolved = resolve_material_for_layer(&part_id, &layer);

        assert_eq!(resolved.part_id, part_id);
        assert_eq!(resolved.albedo_path, layer.asset_path);
        assert!(resolved.mask_path.is_none());
        assert!(resolved.normal_path.is_none());
        assert!(resolved.shadow.is_none());
        assert!(resolved.palette.is_empty());
        assert_eq!(resolved.depth_offset, 0.0);
        assert_eq!(resolved.highlight, 0.0);
    }

    #[test]
    fn renderer_settings_stay_bounded_when_resolution_is_called_directly() {
        let (part_id, layer) = layer_with(MaterialMetadata {
            shadow_path: Some("fighters/human/runtime/torso_shadow.png".to_owned()),
            depth_offset: Some(f32::INFINITY),
            highlight: Some(9.0),
            ..Default::default()
        });

        let resolved = resolve_material_for_layer(&part_id, &layer);

        assert!((-1.0..=1.0).contains(&resolved.depth_offset));
        assert!((0.0..=1.0).contains(&resolved.highlight));
        assert!((0.0..=0.35).contains(&resolved.shadow.expect("shadow is present").strength));
    }

    #[test]
    fn hybrid_material_uses_a_crisp_alpha_cutout() {
        let material = HybridCharacterMaterial {
            uniforms: HybridCharacterUniforms {
                tint: Vec4::ONE,
                palette_0: Vec4::ZERO,
                palette_1: Vec4::ZERO,
                palette_2: Vec4::ZERO,
                settings: Vec4::ZERO,
                render_flags: Vec4::ZERO,
            },
            albedo: Handle::default(),
            mask: Handle::default(),
            normal: Handle::default(),
            shadow: Handle::default(),
        };

        assert_eq!(material.alpha_mode(), AlphaMode2d::Mask(0.5));
    }

    #[test]
    fn pending_material_promotes_only_after_all_images_exist() {
        let images = Assets::<Image>::default();
        let albedo = images.reserve_handle();
        let mask = images.reserve_handle();
        let normal = images.reserve_handle();
        let shadow = images.reserve_handle();

        let mut app = App::new();
        app.insert_resource(images);
        app.insert_resource(Assets::<Mesh>::default());
        app.insert_resource(Assets::<HybridCharacterMaterial>::default());
        app.add_systems(Update, promote_ready_hybrid_materials);

        let size = Vec2::new(44.0, 74.0);
        let transform =
            Transform::from_xyz(3.0, 5.0, 0.14).with_rotation(Quat::from_rotation_z(0.2));
        let (part_id, layer) = layer_with(MaterialMetadata {
            mask_path: Some("fighters/human/runtime/torso_mask.png".to_owned()),
            normal_path: Some("fighters/human/runtime/torso_normal.png".to_owned()),
            shadow_path: Some("fighters/human/runtime/torso_shadow.png".to_owned()),
            palette: vec![PaletteRegion::Cloth],
            depth_offset: Some(0.25),
            highlight: Some(0.5),
        });
        let resolved = resolve_material_for_layer(&part_id, &layer);
        let entity = app
            .world_mut()
            .spawn((
                Sprite::from_color(Color::srgb(0.7, 0.2, 0.2), size),
                transform,
                PendingHybridCharacterMaterial {
                    resolved,
                    albedo: albedo.clone(),
                    mask: mask.clone(),
                    normal: normal.clone(),
                    shadow: shadow.clone(),
                    size,
                    color: Color::srgb(0.7, 0.2, 0.2),
                    flip_x: true,
                },
            ))
            .id();

        app.update();
        assert!(
            app.world().get::<Sprite>(entity).is_some(),
            "the albedo sprite remains visible while channels are pending"
        );
        assert!(app.world().get::<Mesh2d>(entity).is_none());

        {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            for handle in [&albedo, &mask, &normal, &shadow] {
                images
                    .insert(handle.id(), Image::default())
                    .expect("reserved image handle accepts its asset");
            }
        }

        app.update();
        assert!(app.world().get::<Sprite>(entity).is_none());
        assert!(app.world().get::<Mesh2d>(entity).is_some());
        let material_handle = app
            .world()
            .get::<MeshMaterial2d<HybridCharacterMaterial>>(entity)
            .expect("loaded channels promote to the hybrid material");
        let material = app
            .world()
            .resource::<Assets<HybridCharacterMaterial>>()
            .get(material_handle.id())
            .expect("promoted material asset exists");
        assert_eq!(material.uniforms.render_flags.x, 1.0);
        assert_eq!(material.uniforms.settings.x, 0.25);
        assert_eq!(material.uniforms.settings.y, 0.5);
        assert_eq!(material.uniforms.settings.w, 1.0);
        assert_eq!(*app.world().get::<Transform>(entity).unwrap(), transform);
        assert!(
            app.world()
                .get::<PendingHybridCharacterMaterial>(entity)
                .is_none()
        );
    }
}
