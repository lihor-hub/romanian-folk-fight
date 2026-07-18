#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct HybridCharacterUniforms {
    tint: vec4<f32>,
    palette_0: vec4<f32>,
    palette_1: vec4<f32>,
    palette_2: vec4<f32>,
    palette_3: vec4<f32>,
    // x: depth offset, y: highlight, z: contact-shadow strength,
    // w: active palette-channel count.
    settings: vec4<f32>,
    // x: horizontal UV mirror flag.
    render_flags: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: HybridCharacterUniforms;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var albedo_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var mask_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var shadow_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var shadow_sampler: sampler;

fn pixel_coord(uv: vec2<f32>, dimensions: vec2<u32>) -> vec2<i32> {
    let max_coord = vec2<i32>(dimensions) - vec2<i32>(1, 1);
    let sampled = vec2<i32>(floor(clamp(uv, vec2(0.0), vec2(1.0)) * vec2<f32>(dimensions)));
    return clamp(sampled, vec2<i32>(0, 0), max_coord);
}

fn apply_palette(base: vec3<f32>, mask: vec4<f32>, count: f32) -> vec3<f32> {
    var color = base;
    if count >= 1.0 {
        color = mix(color, material.palette_0.rgb, clamp(mask.r, 0.0, 1.0));
    }
    if count >= 2.0 {
        color = mix(color, material.palette_1.rgb, clamp(mask.g, 0.0, 1.0));
    }
    if count >= 3.0 {
        color = mix(color, material.palette_2.rgb, clamp(mask.b, 0.0, 1.0));
    }
    if count >= 4.0 {
        color = mix(color, material.palette_3.rgb, clamp(mask.a, 0.0, 1.0));
    }
    return color;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var uv = mesh.uv;
    if material.render_flags.x > 0.5 {
        uv.x = 1.0 - uv.x;
    }

    // Integer texel loads keep authored pixel edges independent of sampler
    // filtering and screen scale.
    let albedo = textureLoad(
        albedo_texture,
        pixel_coord(uv, textureDimensions(albedo_texture)),
        0,
    );
    let alpha = albedo.a * material.tint.a;
    if alpha < 0.5 {
        discard;
    }

    let mask = textureLoad(
        mask_texture,
        pixel_coord(uv, textureDimensions(mask_texture)),
        0,
    );
    var base = albedo.rgb * material.tint.rgb;
    base = apply_palette(base, mask, material.settings.w);

    var sampled_normal = textureLoad(
        normal_texture,
        pixel_coord(uv, textureDimensions(normal_texture)),
        0,
    ).xyz * 2.0 - vec3(1.0);
    if material.render_flags.x > 0.5 {
        sampled_normal.x = -sampled_normal.x;
    }
    let normal_length = length(sampled_normal);
    var normal = sampled_normal / max(normal_length, 0.001);
    if normal_length < 0.001 {
        normal = vec3(0.0, 0.0, 1.0);
    }
    let light_direction = normalize(vec3(-0.35, 0.45, 0.82));
    let diffuse = max(dot(normal, light_direction), 0.0);
    let highlight = clamp(material.settings.y, 0.0, 1.0);
    let lighting = mix(0.88, 1.0 + 0.18 * highlight, diffuse);

    let depth = clamp(material.settings.x, -1.0, 1.0);
    let shadow_uv = uv + vec2(-0.004, -0.006) * depth;
    let authored_shadow = textureLoad(
        shadow_texture,
        pixel_coord(shadow_uv, textureDimensions(shadow_texture)),
        0,
    ).r;
    let shadow_strength = clamp(material.settings.z, 0.0, 0.35);
    let contact_shadow = 1.0 - shadow_strength * (1.0 - authored_shadow);

    return vec4(base * lighting * contact_shadow, 1.0);
}
