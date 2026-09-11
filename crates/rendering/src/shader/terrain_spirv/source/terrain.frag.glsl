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
layout(set = 2, binding = 1) uniform sampler2D primary_shadow_map;
layout(set = 2, binding = 2) uniform sampler2D environment_shadow_0;
layout(set = 2, binding = 3) uniform sampler2D environment_shadow_1;
layout(set = 2, binding = 4) uniform sampler2D environment_shadow_2;
layout(location = 5) in vec3 in_shadow_coordinates;
layout(location = 10) in vec3 environment_coordinates[3];

// The direct-depth kernels use c3..10 (terrain) or c5..12 (MapObj/detail).
float filtered_shadow(sampler2D map, vec3 coordinates, int step_size) {
    const vec2 offsets[8] = vec2[8](
        vec2(0.8, -1.0), vec2(-0.2, -0.8), vec2(0.2, -0.6), vec2(1.0, -0.4),
        vec2(-0.6, -0.2), vec2(0.6, 0.2), vec2(-1.0, -0.4), vec2(-0.4, -0.6));
    vec2 uv = coordinates.xy * 0.5 + vec2(0.5);
    float visibility = float(textureLod(map, uv, 0.0).r >= coordinates.z);
    for (int index = 0; index < 8; index += step_size) {
        visibility += float(textureLod(map,
            uv + offsets[index] * shadow.origin_and_texel.w, 0.0).r >= coordinates.z);
    }
    return visibility / (step_size == 2 ? 5.0 : 9.0);
}

// Original PS3 selects the first containing environment map and fades the far edge.
float environment_shadow_visibility() {
    if (max(abs(environment_coordinates[0].x), abs(environment_coordinates[0].y)) < 1.0) {
        return filtered_shadow(environment_shadow_0, environment_coordinates[0], 2);
    }
    if (max(abs(environment_coordinates[1].x), abs(environment_coordinates[1].y)) < 1.0) {
        return filtered_shadow(environment_shadow_1, environment_coordinates[1], 2);
    }
    float visibility = filtered_shadow(environment_shadow_2, environment_coordinates[2], 2);
    float edge = clamp(max(abs(environment_coordinates[2].x), abs(environment_coordinates[2].y))
        * -11.1111107 + 11.0, 0.0, 1.0);
    return mix(1.0, visibility, edge);
}

// Terrain3 PS8 takes the minimum; PS16 selects the primary nine-tap result
// without fading whenever its map contains the receiver. Both omit baked MCSH.
float world_shadow_visibility(float baked_visibility) {
    float edge = clamp(max(abs(in_shadow_coordinates.x), abs(in_shadow_coordinates.y))
        * -3.4482758 + 3.41379309, 0.0, 1.0);
    int mode = int(shadow.settings.x);
    if (mode == 3) {
        return edge > 0.0 ? filtered_shadow(primary_shadow_map, in_shadow_coordinates, 1)
            : environment_shadow_visibility();
    }
    float primary = edge > 0.0
        ? mix(1.0, filtered_shadow(primary_shadow_map, in_shadow_coordinates, 2), edge) : 1.0;
    return min(primary, mode == 2 ? environment_shadow_visibility() : baked_visibility);
}
#endif

#ifndef TERRAIN_LAYER_COUNT
#error TERRAIN_LAYER_COUNT must be defined
#endif

#if TERRAIN_LAYER_COUNT < 1 || TERRAIN_LAYER_COUNT > 4
#error TERRAIN_LAYER_COUNT lies outside stock's one-through-four range
#endif

layout(location = 1) in vec2 in_texture_coordinates;
layout(location = 2) in vec2 in_atlas_coordinates;
layout(location = 3) in vec3 in_vertex_light;
layout(location = 4) in float in_fog_visibility;
layout(location = 6) in vec3 in_vertex_specular;
#if TERRAIN_LAYER_COUNT > 1
layout(location = 7) in vec2 in_layer1_coordinates;
#endif
#if TERRAIN_LAYER_COUNT > 2
layout(location = 8) in vec2 in_layer2_coordinates;
#endif
#if TERRAIN_LAYER_COUNT > 3
layout(location = 9) in vec2 in_layer3_coordinates;
#endif

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

layout(set = 1, binding = 0) uniform sampler2D material_atlas;
layout(set = 1, binding = 1) uniform sampler2D diffuse_0;
#if TERRAIN_LAYER_COUNT > 1
layout(set = 1, binding = 2) uniform sampler2D diffuse_1;
#endif
#if TERRAIN_LAYER_COUNT > 2
layout(set = 1, binding = 3) uniform sampler2D diffuse_2;
#endif
#if TERRAIN_LAYER_COUNT > 3
layout(set = 1, binding = 4) uniform sampler2D diffuse_3;
#endif

layout(location = 0) out vec4 out_color;

vec4 light_layer(vec4 color, uint layer) {
    // Terrain1 PS16..31 applies diffuse lighting before mixing an unlit layer.
    // Alpha remains the independent specular mask for both kinds of layer.
    if (draw.unlit_layers != 0 && (draw.unlit_layers & (1u << layer)) == 0) {
        color.rgb *= in_vertex_light * 2.0;
    }
    return color;
}

void main() {
    vec4 material = texture(material_atlas, in_atlas_coordinates);
    vec4 ground = light_layer(texture(diffuse_0, in_texture_coordinates), 0);
    if (draw.weighted_blending != 0) {
        // Terrain1 PS8..15/24..31 saturates the sum, never the other weights.
        ground *= 1.0 - clamp(dot(material.rgb, vec3(1.0)), 0.0, 1.0);
    }
#if TERRAIN_LAYER_COUNT > 1
    vec4 layer1 = light_layer(texture(diffuse_1, in_layer1_coordinates), 1);
    ground = draw.weighted_blending != 0 ? ground + layer1 * material.r
        : mix(ground, layer1, material.r);
#endif
#if TERRAIN_LAYER_COUNT > 2
    vec4 layer2 = light_layer(texture(diffuse_2, in_layer2_coordinates), 2);
    ground = draw.weighted_blending != 0 ? ground + layer2 * material.g
        : mix(ground, layer2, material.g);
#endif
#if TERRAIN_LAYER_COUNT > 3
    vec4 layer3 = light_layer(texture(diffuse_3, in_layer3_coordinates), 3);
    ground = draw.weighted_blending != 0 ? ground + layer3 * material.b
        : mix(ground, layer3, material.b);
#endif

    // Terrain1.bls multiplies shadow visibility by 0.3 and adds 0.7;
    // diffuse vertex colors are doubled after texture/shadow multiplication.
    float visibility = 1.0 - material.a;
#if TERRAIN_PRIMARY_SHADOW
    visibility = world_shadow_visibility(visibility);
#endif
    float baked_shadow = 0.7 + 0.3 * visibility;
    vec3 lit = ground.rgb * baked_shadow;
    if (draw.unlit_layers == 0) {
        lit *= in_vertex_light * 2.0;
    }
    lit += ground.a * in_vertex_specular * visibility;
    out_color = vec4(mix(scene.fog_color.rgb, lit, in_fog_visibility), 1.0);
}
