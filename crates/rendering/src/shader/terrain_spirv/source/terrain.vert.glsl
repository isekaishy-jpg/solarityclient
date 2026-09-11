#version 460

#ifndef TERRAIN_PRIMARY_SHADOW
#define TERRAIN_PRIMARY_SHADOW 0
#endif

#if TERRAIN_PRIMARY_SHADOW
layout(set = 2, binding = 0) uniform TerrainShadow {
    vec4 origin_and_texel;
    vec4 receiver_rows[3];
    vec4 light_direction;
    vec4 environment_rows[9];
    vec4 fade_plane;
    vec4 settings;
} shadow;
layout(location = 5) out vec3 out_shadow_coordinates;
layout(location = 10) out vec3 environment_coordinates[3];
#endif

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec2 in_texture_coordinates;
layout(location = 3) in vec2 in_alpha_coordinates;
layout(location = 4) in vec3 in_color_rgb;

layout(set = 0, binding = 0) uniform TerrainScene {
    mat4 projection;
    vec4 ambient_color;
    vec4 diffuse_color;
    vec4 sun_direction;
    vec4 view_depth;
    vec4 fog_parameters;
    vec4 fog_color;
    mat4 view;
    vec4 specular_color_and_power;
    vec4 texture_offsets[64];
} scene;

// Scalar members keep each native nine-float light group at 36 bytes in
// std430, fitting all three beside the material data in Vulkan's 128-byte minimum.
struct TerrainPointLight {
    float x; float y; float z;
    float red; float green; float blue;
    float constant_term; float linear_term; float quadratic_term;
};

layout(push_constant) uniform TerrainDraw {
    uvec2 atlas_chunk;
    uint weighted_blending;
    uint unlit_layers;
    uint texture_animation;
    TerrainPointLight point_lights[3];
} draw;

layout(location = 1) out vec2 out_texture_coordinates;
layout(location = 2) out vec2 out_atlas_coordinates;
layout(location = 3) out vec3 out_vertex_light;
layout(location = 4) out float out_fog_visibility;
layout(location = 6) out vec3 out_vertex_specular;
#if TERRAIN_LAYER_COUNT > 1
layout(location = 7) out vec2 out_layer1_coordinates;
#endif
#if TERRAIN_LAYER_COUNT > 2
layout(location = 8) out vec2 out_layer2_coordinates;
#endif
#if TERRAIN_LAYER_COUNT > 3
layout(location = 9) out vec2 out_layer3_coordinates;
#endif

vec2 layer_coordinates(uint layer) {
    uint flags = (draw.texture_animation >> (layer * 8u)) & 127u;
    return in_texture_coordinates + ((flags & 64u) != 0u
        ? scene.texture_offsets[flags & 63u].xy : vec2(0.0));
}

void main() {
    const float chunk_texels = 64.0;
    const float atlas_texels = 1024.0;
    vec2 atlas_origin = vec2(draw.atlas_chunk) * chunk_texels;

    // Terrain.bls uses c0..c3 for view, then c4..c7 for projection. Keeping
    // clip Z separate prevents depth cancellation at large world coordinates.
    precise vec4 view_position = scene.view * vec4(in_position, 1.0);
    gl_Position = scene.projection * view_position;
    // Original Terrain.bls uses view Z and c12, then pow/max/min into oFog.
    float eye_depth = -dot(scene.view_depth, vec4(in_position, 1.0));
    float inverse_range = 1.0 / (scene.fog_parameters.y - scene.fog_parameters.x);
    float visibility = max((scene.fog_parameters.y - eye_depth) * inverse_range, 0.0);
    out_fog_visibility = scene.fog_color.w > 0.0
        ? min(pow(visibility, scene.fog_parameters.w), 1.0) : 1.0;
    out_texture_coordinates = layer_coordinates(0);
#if TERRAIN_LAYER_COUNT > 1
    out_layer1_coordinates = layer_coordinates(1);
#endif
#if TERRAIN_LAYER_COUNT > 2
    out_layer2_coordinates = layer_coordinates(2);
#endif
#if TERRAIN_LAYER_COUNT > 3
    out_layer3_coordinates = layer_coordinates(3);
#endif
    out_atlas_coordinates = (atlas_origin + in_alpha_coordinates * chunk_texels) / atlas_texels;

    // Terrain.bls lights vertices before interpolation. The normal and light
    // direction are transformed by the same camera rotation in the original.
    float diffuse_amount = clamp(dot(in_normal, scene.sun_direction.xyz), 0.0, 1.0);
    vec3 lighting = scene.ambient_color.rgb + scene.diffuse_color.rgb * diffuse_amount;
    for (uint index = 0; index < 3; ++index) {
        TerrainPointLight point = draw.point_lights[index];
        vec3 color = vec3(point.red, point.green, point.blue);
        if (any(notEqual(color, vec3(0.0)))) {
            // Terrain.bls VS64..127 transforms camera-relative light positions,
            // then adds diffuse-only attenuation before the final color clamp.
            vec3 delta = mat3(scene.view) * vec3(point.x, point.y, point.z) - view_position.xyz;
            float distance_to_light = length(delta);
            vec3 normal = mat3(scene.view) * in_normal;
            float amount = clamp(dot(normal, delta / distance_to_light), 0.0, 1.0);
            float denominator = point.constant_term + point.linear_term * distance_to_light
                + point.quadratic_term * distance_to_light * distance_to_light;
            lighting += color * amount / denominator;
        }
    }
    lighting = min(lighting, vec3(1.0));
    out_vertex_light = lighting * in_color_rgb;
    out_vertex_specular = vec3(0.0);
    if (scene.specular_color_and_power.w > 0.0) {
        // Terrain.bls specular variants use a normalized view/light half
        // vector and the unnormalized transformed MCNR normal. MCCV only
        // modulates diffuse lighting, never the independent oD1 highlight.
        vec3 view_direction = normalize(-view_position.xyz);
        vec3 light_direction = mat3(scene.view) * scene.sun_direction.xyz;
        vec3 half_direction = normalize(view_direction + light_direction);
        vec3 view_normal = mat3(scene.view) * in_normal;
        float amount = pow(max(dot(half_direction, view_normal), 0.0),
            scene.specular_color_and_power.w);
        // VS2 oD1 clamps color before interpolation, independently of the
        // later BLP alpha mask and shadow factor in Terrain1/2/3.
        out_vertex_specular = clamp(scene.specular_color_and_power.rgb * amount, 0.0, 1.0);
    }
#if TERRAIN_PRIMARY_SHADOW
    vec4 relative_position = vec4(in_position - shadow.origin_and_texel.xyz, 1.0);
    out_shadow_coordinates = vec3(
        dot(relative_position, shadow.receiver_rows[0]),
        dot(relative_position, shadow.receiver_rows[1]),
        dot(relative_position, shadow.receiver_rows[2]));
    for (int map = 0; map < 3; ++map) {
        environment_coordinates[map] = vec3(
            dot(relative_position, shadow.environment_rows[map * 3]),
            dot(relative_position, shadow.environment_rows[map * 3 + 1]),
            dot(relative_position, shadow.environment_rows[map * 3 + 2]));
    }
#endif
}
