#version 460

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

layout(set = 0, binding = 0) uniform TerrainScene {
    mat4 projection;
    vec4 ambient_color;
    vec4 diffuse_color;
    vec4 sun_direction;
    vec4 view_depth;
    vec4 fog_parameters;
    vec4 fog_color;
    mat4 view;
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
    vec3 ground = texture(diffuse_0, in_texture_coordinates).rgb;
#if TERRAIN_LAYER_COUNT > 1
    ground = mix(ground, texture(diffuse_1, in_texture_coordinates).rgb, material.r);
#endif
#if TERRAIN_LAYER_COUNT > 2
    ground = mix(ground, texture(diffuse_2, in_texture_coordinates).rgb, material.g);
#endif
#if TERRAIN_LAYER_COUNT > 3
    ground = mix(ground, texture(diffuse_3, in_texture_coordinates).rgb, material.b);
#endif

    // Terrain1.bls multiplies shadow visibility by 0.3 and adds 0.7;
    // diffuse vertex colors are doubled after texture/shadow multiplication.
    float baked_shadow = 0.7 + 0.3 * (1.0 - material.a);
    vec3 lit = ground * baked_shadow * in_vertex_light * 2.0;
    out_color = vec4(mix(scene.fog_color.rgb, lit, in_fog_visibility), 1.0);
}
