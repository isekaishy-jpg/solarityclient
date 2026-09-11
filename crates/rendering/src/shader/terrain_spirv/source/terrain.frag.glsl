#version 460

#ifndef TERRAIN_PRIMARY_SHADOW
#define TERRAIN_PRIMARY_SHADOW 0
#endif

#if TERRAIN_PRIMARY_SHADOW
layout(set = 2, binding = 0) uniform TerrainShadow {
    vec4 origin_and_texel;
    vec4 receiver_rows[3];
} shadow;
layout(set = 2, binding = 1) uniform sampler2D primary_shadow_map;
layout(location = 5) in vec3 in_shadow_coordinates;

// Terrain2/Terrain3's primary map uses the center and four odd filter registers.
float primary_shadow_visibility() {
    float edge = clamp(max(abs(in_shadow_coordinates.x), abs(in_shadow_coordinates.y))
        * -3.4482758 + 3.41379309, 0.0, 1.0);
    if (edge <= 0.0) {
        return 1.0;
    }
    vec2 coordinates = in_shadow_coordinates.xy * 0.5 + vec2(0.5);
    vec2 offsets[5] = vec2[5](vec2(0.0), vec2(0.8, -1.0), vec2(0.2, -0.6),
        vec2(-0.6, -0.2), vec2(-1.0, -0.4));
    float visibility = 0.0;
    for (int index = 0; index < 5; ++index) {
        float depth = textureLod(primary_shadow_map,
            coordinates + offsets[index] * shadow.origin_and_texel.w, 0.0).r;
        visibility += depth >= in_shadow_coordinates.z ? 1.0 : 0.0;
    }
    return 1.0 + edge * (visibility * 0.2 - 1.0);
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

void main() {
    vec4 material = texture(material_atlas, in_atlas_coordinates);
    vec4 ground = texture(diffuse_0, in_texture_coordinates);
#if TERRAIN_LAYER_COUNT > 1
    ground = mix(ground, texture(diffuse_1, in_texture_coordinates), material.r);
#endif
#if TERRAIN_LAYER_COUNT > 2
    ground = mix(ground, texture(diffuse_2, in_texture_coordinates), material.g);
#endif
#if TERRAIN_LAYER_COUNT > 3
    ground = mix(ground, texture(diffuse_3, in_texture_coordinates), material.b);
#endif

    // Terrain1.bls multiplies shadow visibility by 0.3 and adds 0.7;
    // diffuse vertex colors are doubled after texture/shadow multiplication.
    float visibility = 1.0 - material.a;
#if TERRAIN_PRIMARY_SHADOW
    visibility = min(visibility, primary_shadow_visibility());
#endif
    float baked_shadow = 0.7 + 0.3 * visibility;
    vec3 lit = ground.rgb * baked_shadow * in_vertex_light * 2.0
        + ground.a * in_vertex_specular * visibility;
    out_color = vec4(mix(scene.fog_color.rgb, lit, in_fog_visibility), 1.0);
}
